//! Minimal in-process event bus for decoupled operational event emission.
//! M3 scope: bounded queue + background log sink, no persistence/replay yet.

const std = @import("std");
const log = std.log.scoped(.event_bus);

const CAPACITY: usize = 1024;
const KIND_MAX: usize = 32;
const RUN_ID_MAX: usize = 64;
const DETAIL_MAX: usize = 256;

pub const BusEvent = struct {
    ts_ms: i64 = 0,
    kind: [KIND_MAX]u8 = undefined,
    kind_len: u8 = 0,
    run_id: [RUN_ID_MAX]u8 = undefined,
    run_id_len: u8 = 0,
    detail: [DETAIL_MAX]u8 = undefined,
    detail_len: u16 = 0,

    pub fn init(kind: []const u8, run_id: ?[]const u8, detail: []const u8) BusEvent {
        var out = BusEvent{
            .ts_ms = std.time.milliTimestamp(),
        };
        out.kind_len = @intCast(copyTrunc(&out.kind, kind));
        out.run_id_len = @intCast(copyTrunc(&out.run_id, run_id orelse ""));
        out.detail_len = @intCast(copyTrunc(&out.detail, detail));
        return out;
    }

    pub fn kindSlice(self: *const BusEvent) []const u8 {
        return self.kind[0..@as(usize, self.kind_len)];
    }

    pub fn runIdSlice(self: *const BusEvent) []const u8 {
        return self.run_id[0..@as(usize, self.run_id_len)];
    }

    pub fn detailSlice(self: *const BusEvent) []const u8 {
        return self.detail[0..@as(usize, self.detail_len)];
    }
};

pub const Bus = struct {
    mutex: std.Thread.Mutex = .{},
    cond: std.Thread.Condition = .{},
    running: std.atomic.Value(bool) = std.atomic.Value(bool).init(true),
    queue: [CAPACITY]BusEvent = undefined,
    head: usize = 0,
    tail: usize = 0,
    len: usize = 0,
    dropped: u64 = 0,

    pub fn init() Bus {
        return .{};
    }

    pub fn stop(self: *Bus) void {
        self.running.store(false, .release);
        self.cond.broadcast();
    }

    pub fn pendingCount(self: *Bus) usize {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.len;
    }

    pub fn droppedCount(self: *Bus) u64 {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.dropped;
    }

    pub fn publish(self: *Bus, event: BusEvent) void {
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

    fn popLocked(self: *Bus) BusEvent {
        const event = self.queue[self.head];
        self.head = (self.head + 1) % CAPACITY;
        self.len -= 1;
        return event;
    }

    pub fn run(self: *Bus) void {
        while (true) {
            self.mutex.lock();
            while (self.len == 0 and self.running.load(.acquire)) {
                self.cond.wait(&self.mutex);
            }

            if (self.len == 0 and !self.running.load(.acquire)) {
                self.mutex.unlock();
                break;
            }

            const event = self.popLocked();
            const dropped = self.dropped;
            self.dropped = 0;
            self.mutex.unlock();

            if (dropped > 0) {
                log.warn("event.dropped count={d}", .{dropped});
            }

            const run_id = if (event.runIdSlice().len == 0) "-" else event.runIdSlice();
            log.info("event.emitted ts_ms={d} kind={s} run_id={s} detail={s}", .{
                event.ts_ms,
                event.kindSlice(),
                run_id,
                event.detailSlice(),
            });
        }
    }
};

var global_bus = std.atomic.Value(?*Bus).init(null);

pub fn install(bus: *Bus) void {
    const previous = global_bus.swap(bus, .acq_rel);
    std.debug.assert(previous == null);
}

pub fn uninstall() void {
    _ = global_bus.swap(null, .acq_rel);
}

pub fn emit(kind: []const u8, run_id: ?[]const u8, detail: []const u8) void {
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
    std.Thread.sleep(5 * std.time.ns_per_ms);
    bus.stop();
    thread.join();

    try std.testing.expectEqual(@as(usize, 0), bus.pendingCount());
}

test "integration: event bus drains queued events before shutdown completes" {
    var bus = Bus.init();
    const thread = try std.Thread.spawn(.{}, runThread, .{&bus});

    bus.publish(BusEvent.init("k1", "run-1", "d1"));
    bus.publish(BusEvent.init("k2", "run-2", "d2"));
    std.Thread.sleep(5 * std.time.ns_per_ms);
    bus.stop();
    thread.join();

    try std.testing.expectEqual(@as(usize, 0), bus.pendingCount());
    try std.testing.expectEqual(@as(u64, 0), bus.droppedCount());
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
