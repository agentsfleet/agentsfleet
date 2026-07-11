//! Minimal in-process event bus for decoupled operational event emission.
//! Bounded queue + background log sink, no persistence/replay yet.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");

const log = logging.scoped(.event_bus);

const CAPACITY: usize = 1024;
const KIND_MAX: usize = 32;
const RUN_ID_MAX: usize = 64;
const DETAIL_MAX: usize = 256;

const BusEvent = struct {
    const Self = @This();

    ts_ms: i64 = 0,
    // SAFETY: populated by the owning init/builder before any consumer reads this field.
    kind: [KIND_MAX]u8 = undefined,
    kind_len: u8 = 0,
    // SAFETY: populated by the owning init/builder before any consumer reads this field.
    run_id: [RUN_ID_MAX]u8 = undefined,
    run_id_len: u8 = 0,
    // SAFETY: populated by the owning init/builder before any consumer reads this field.
    detail: [DETAIL_MAX]u8 = undefined,
    detail_len: u16 = 0,

    pub fn init(kind: []const u8, run_id: ?[]const u8, detail: []const u8) BusEvent {
        var out = BusEvent{
            .ts_ms = clock.nowMillis(),
        };
        out.kind_len = @intCast(copyTrunc(&out.kind, kind));
        out.run_id_len = @intCast(copyTrunc(&out.run_id, run_id orelse ""));
        out.detail_len = @intCast(copyTrunc(&out.detail, detail));
        return out;
    }

    fn kindSlice(self: *const Self) []const u8 {
        return self.kind[0..@as(usize, self.kind_len)];
    }

    fn runIdSlice(self: *const Self) []const u8 {
        return self.run_id[0..@as(usize, self.run_id_len)];
    }

    fn detailSlice(self: *const Self) []const u8 {
        return self.detail[0..@as(usize, self.detail_len)];
    }
};

pub const Bus = struct {
    const Self = @This();

    /// Guards the event queue; orders a producer push against the consumer's predicate check.
    mutex: common.Mutex = .{},
    cond: common.Condition = .{},
    running: std.atomic.Value(bool) = std.atomic.Value(bool).init(true),
    // SAFETY: populated by the owning init/builder before any consumer reads this field.
    queue: [CAPACITY]BusEvent = undefined,
    head: usize = 0,
    tail: usize = 0,
    len: usize = 0,
    dropped: u64 = 0,

    pub fn init() Bus {
        return .{};
    }

    pub fn stop(self: *Self) void {
        self.mutex.lock();
        // safe because: the .release store pairs with waitNext's .acquire loads; holding
        // the mutex orders it against the consumer's predicate check — the consumer is
        // either already parked (the broadcast below wakes it) or re-checks after unlock
        // and sees running == false. An unlocked store can land between predicate check
        // and waiter registration: broadcast then has no waiter and the join hangs.
        self.running.store(false, .release);
        self.mutex.unlock();
        self.cond.broadcast();
    }

    pub fn pendingCount(self: *Self) usize {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.len;
    }

    fn droppedCount(self: *Self) u64 {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.dropped;
    }

    pub fn publish(self: *Self, event: BusEvent) void {
        self.mutex.lock();
        defer self.mutex.unlock();

        if (self.len >= CAPACITY) {
            self.dropped += 1;
            return;
        }

        self.queue[self.tail] = event;
        self.tail = (self.tail + 1) % CAPACITY;
        self.len += 1;
        self.cond.signal();
    }

    fn popLocked(self: *Self) BusEvent {
        const event = self.queue[self.head];
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        return event;
    }

    const NextEvent = struct {
        event: BusEvent,
        dropped: u64,
    };

    fn waitNext(self: *Self) ?NextEvent {
        self.mutex.lock();
        defer self.mutex.unlock();
        // safe because: both .acquire loads here pair with stop()'s mutex-ordered .release store.
        while (self.len == 0 and self.running.load(.acquire)) {
            self.cond.wait(&self.mutex);
        }
        if (self.len == 0 and !self.running.load(.acquire)) return null;
        const event = self.popLocked();
        const dropped = self.dropped;
        self.dropped = 0;
        return .{ .event = event, .dropped = dropped };
    }

    pub fn run(self: *Self) void {
        while (self.waitNext()) |next| {
            if (next.dropped > 0) {
                log.warn("dropped", .{ .count = next.dropped });
            }
            const run_id = if (next.event.runIdSlice().len == 0) "-" else next.event.runIdSlice();
            log.info("emitted", .{ .ts_ms = next.event.ts_ms, .kind = next.event.kindSlice(), .run_id = run_id, .detail = next.event.detailSlice() });
        }
    }
};

var global_bus = std.atomic.Value(?*Bus).init(null);

pub fn install(bus: *Bus) void {
    // safe because: .acq_rel publishes the fully-initialized bus to emit()'s .acquire load.
    const previous = global_bus.swap(bus, .acq_rel);
    std.debug.assert(previous == null);
}

pub fn uninstall() void {
    // safe because: .acq_rel pairs with emit()'s .acquire load; later emits see null and drop.
    _ = global_bus.swap(null, .acq_rel);
}

pub fn emit(kind: []const u8, run_id: ?[]const u8, detail: []const u8) void {
    // safe because: .acquire pairs with install/uninstall's .acq_rel swaps.
    const bus = global_bus.load(.acquire) orelse return;
    bus.publish(BusEvent.init(kind, run_id, detail));
}

pub fn runThread(bus: *Bus) void {
    bus.run();
}

fn copyTrunc(dst_ptr: anytype, src: []const u8) usize {
    const dst: []u8 = dst_ptr[0..];
    const n = @min(dst.len, src.len);
    @memcpy(dst[0..n], src[0..n]);
    return n;
}

test "event slices are truncated to bounded limits" {
    const event = BusEvent.init(
        "this_kind_value_is_longer_than_allowed",
        "run_id_that_is_longer_than_the_bus_event_run_id_capacity_for_testing",
        "detail text that is intentionally larger than expected? no, but enough for path testing",
    );

    try std.testing.expect(event.kindSlice().len <= KIND_MAX);
    try std.testing.expect(event.runIdSlice().len <= RUN_ID_MAX);
    try std.testing.expect(event.detailSlice().len <= DETAIL_MAX);
}

test "integration: event bus run thread exits when stopped while idle" {
    var bus = Bus.init();
    const thread = try std.Thread.spawn(.{}, runThread, .{&bus});
    common.sleepNanos(5 * std.time.ns_per_ms);
    bus.stop();
    thread.join();

    try std.testing.expectEqual(@as(usize, 0), bus.pendingCount());
}

test "integration: event bus drains queued events before shutdown completes" {
    var bus = Bus.init();
    const thread = try std.Thread.spawn(.{}, runThread, .{&bus});

    bus.publish(BusEvent.init("k1", "run-1", "d1"));
    bus.publish(BusEvent.init("k2", "run-2", "d2"));
    common.sleepNanos(5 * std.time.ns_per_ms);
    bus.stop();
    thread.join();

    try std.testing.expectEqual(@as(usize, 0), bus.pendingCount());
    try std.testing.expectEqual(@as(u64, 0), bus.droppedCount());
}

test "integration: stop never loses the wakeup under repeated start/stop" {
    // Rolls the predicate-check-vs-stop window with no sleeps; a lost wakeup
    // would hang this join and time the test out.
    var i: usize = 0;
    while (i < 200) : (i += 1) {
        var bus = Bus.init();
        const thread = try std.Thread.spawn(.{}, runThread, .{&bus});
        bus.publish(BusEvent.init("k", "r", "d"));
        bus.stop();
        thread.join();
        try std.testing.expectEqual(@as(usize, 0), bus.pendingCount());
    }
}

test "integration: emit is ignored after uninstall" {
    var bus = Bus.init();
    install(&bus);
    defer uninstall();

    emit("before_uninstall", "run-1", "detail");
    try std.testing.expectEqual(@as(usize, 1), bus.pendingCount());

    uninstall();
    emit("after_uninstall", "run-2", "detail");
    try std.testing.expectEqual(@as(usize, 1), bus.pendingCount());
}
