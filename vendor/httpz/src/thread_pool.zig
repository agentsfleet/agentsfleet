const std = @import("std");

const Io = std.Io;
const Thread = std.Thread;
const Allocator = std.mem.Allocator;

pub const Opts = struct {
    count: u32,
    backlog: u32,
    buffer_size: usize,
};

pub fn ThreadPool(comptime F: anytype) type {
    const BATCH_SIZE = 16;

    // When the worker thread calls F, it'll inject its static buffer.
    // So F would be: handle(server: *Server, conn: *Conn, buf: []u8)
    // and FullArgs would be our 3 args....
    const FullArgs = std.meta.ArgsTuple(@TypeOf(F));
    const Args = SpawnArgs(FullArgs);

    return struct {
        stopped: bool,
        threads: []Thread,
        shared: *Shared,
        arena: std.heap.ArenaAllocator,

        // we queue jobs here before batching them to the shared queue. We do
        // this to minimze the amount of locking we need to do.
        batch: [BATCH_SIZE]Args,
        batch_size: usize,

        const Self = @This();

        // One injector queue shared by every pool thread (vendor patch, see
        // CHANGES.md). Any idle thread takes the next job, so a thread parked
        // inside a long-running job can never strand queued work — the old
        // per-thread private queues + round-robin dispatch black-holed the
        // parked thread's share. Lives in the arena so its address is stable
        // after init returns Self by value (threads hold pointers into it).
        const Shared = struct {
            io: Io,
            mutex: Io.Mutex,
            read_cond: Io.Condition,
            write_cond: Io.Condition,
            queue: []Args,
            // position in queue to read from
            tail: usize,
            // position in the queue to write to
            head: usize,
            stopped: bool,
        };

        // we expect allocator to be an Arena
        pub fn init(io: Io, allocator: Allocator, opts: Opts) !Self {
            var arena = std.heap.ArenaAllocator.init(allocator);
            errdefer arena.deinit();

            const aa = arena.allocator();

            const shared = try aa.create(Shared);
            shared.* = .{
                .io = io,
                .mutex = .init,
                .read_cond = .init,
                .write_cond = .init,
                // the ring buffer always keeps one slot open
                .queue = try aa.alloc(Args, if (opts.backlog < 2) 2 else opts.backlog),
                .tail = 0,
                .head = 0,
                .stopped = false,
            };

            const threads = try aa.alloc(Thread, opts.count);

            var started: usize = 0;
            errdefer {
                {
                    shared.mutex.lockUncancelable(io);
                    defer shared.mutex.unlock(io);
                    shared.stopped = true;
                }
                shared.read_cond.broadcast(io);
                for (threads[0..started]) |*thread| {
                    thread.join();
                }
            }

            for (0..opts.count) |i| {
                const buffer = try aa.alloc(u8, opts.buffer_size);
                threads[i] = try Thread.spawn(.{}, run, .{ shared, buffer });
                started += 1;
            }

            return .{
                .arena = arena,
                .stopped = false,
                .shared = shared,
                .threads = threads,
                .batch = undefined,
                .batch_size = 0,
            };
        }

        pub fn deinit(self: *Self) void {
            self.arena.deinit();
        }

        pub fn stop(self: *Self) void {
            if (@atomicRmw(bool, &self.stopped, .Xchg, true, .monotonic) == true) {
                return;
            }

            const shared = self.shared;
            const io = shared.io;
            {
                shared.mutex.lockUncancelable(io);
                defer shared.mutex.unlock(io);
                shared.stopped = true;
            }
            shared.read_cond.broadcast(io);
            for (self.threads) |*thread| {
                thread.join();
            }
        }

        pub fn spawn(self: *Self, args: Args) void {
            var i = self.batch_size;
            self.batch[i] = args;
            i += 1;

            if (i == BATCH_SIZE) {
                self.flush(i);
                i = 0;
            }
            self.batch_size = i;
        }

        pub fn spawnOne(self: *Self, args: Args) void {
            push(self.shared, &.{args});
        }

        pub fn flush(self: *Self, batch_size: usize) void {
            self.batch_size = 0;
            push(self.shared, self.batch[0..batch_size]);
        }

        pub fn empty(self: *Self) bool {
            const shared = self.shared;
            shared.mutex.lockUncancelable(shared.io);
            defer shared.mutex.unlock(shared.io);
            return shared.head == shared.tail;
        }

        // Queued-but-not-yet-claimed job count. Excludes the producer-side
        // batch staging and jobs currently executing. Callers use this as a
        // load signal (e.g. admission control); it is exact only for the
        // instant the lock was held.
        pub fn pending(self: *Self) usize {
            const shared = self.shared;
            shared.mutex.lockUncancelable(shared.io);
            defer shared.mutex.unlock(shared.io);
            const head = shared.head;
            const tail = shared.tail;
            return if (head >= tail) head - tail else shared.queue.len - tail + head;
        }

        fn push(shared: *Shared, args: []const Args) void {
            const io = shared.io;
            var pending_args = args;

            const queue = shared.queue;
            const queue_end = queue.len - 1;

            while (true) {
                var capacity: usize = 0;
                shared.mutex.lockUncancelable(io);
                var head = shared.head;
                var tail = shared.tail;
                while (true) {
                    capacity = if (head < tail) tail - head - 1 else queue_end - head + tail;
                    if (capacity > 0) {
                        break;
                    }
                    // queue full: block the producer until a consumer frees a
                    // slot (same backpressure as upstream's per-thread ring,
                    // now on the single shared bound).
                    shared.write_cond.waitUncancelable(io, &shared.mutex);
                    head = shared.head;
                    tail = shared.tail;
                }

                const ready = if (capacity >= pending_args.len) pending_args else pending_args[0..capacity];
                for (ready) |a| {
                    queue[head] = a;
                    head = if (head == queue_end) 0 else head + 1;
                }
                shared.head = head;
                shared.mutex.unlock(io);
                // several idle threads may be needed for several jobs
                if (ready.len == 1) shared.read_cond.signal(io) else shared.read_cond.broadcast(io);
                if (ready.len == pending_args.len) {
                    break;
                }
                pending_args = pending_args[ready.len..];
            }
        }

        // Having a re-usable buffer per thread is the most efficient way
        // we can do any dynamic allocations. We'll pair this later with
        // a FallbackAllocator. The main issue is that some data must outlive
        // the worker thread (in nonblocking mode), but this isn't something
        // we need to worry about here. As far as this worker thread is
        // concerned, it has a chunk of memory (buffer) which it'll pass
        // to the callback function to do with as it wants.
        fn run(shared: *Shared, buffer: []u8) void {
            while (getNext(shared)) |args| {
                // convert Args to FullArgs, i.e. inject buffer as the last argument
                var full_args: FullArgs = undefined;
                const ARG_COUNT = std.meta.fields(FullArgs).len - 1;
                full_args[ARG_COUNT] = buffer;
                inline for (0..ARG_COUNT) |i| {
                    full_args[i] = args[i];
                }
                @call(.auto, F, full_args);
            }
        }

        fn getNext(shared: *Shared) ?Args {
            const io = shared.io;
            const queue = shared.queue;
            const queue_end = queue.len - 1;

            shared.mutex.lockUncancelable(io);
            while (shared.tail == shared.head) {
                if (shared.stopped) {
                    // drained: jobs queued before stop() have all been claimed
                    shared.mutex.unlock(io);
                    return null;
                }
                shared.read_cond.waitUncancelable(io, &shared.mutex);
            }

            const tail = shared.tail;
            const args = queue[tail];
            shared.tail = if (tail == queue_end) 0 else tail + 1;
            shared.mutex.unlock(io);
            shared.write_cond.signal(io);
            return args;
        }
    };
}

fn SpawnArgs(FullArgs: anytype) type {
    const full_fields = std.meta.fields(FullArgs);
    const ARG_COUNT = full_fields.len - 1;

    // Args will be FullArgs[0..len-1], so in the above example, args would be
    // (*Server, *Conn)
    // Args is what we expect the caller to pass to spawn. The worker thread
    // will convert an Args into FullArgs by injecting its static buffer as
    // the final argument.

    // TODO: We could verify that the last argument to FullArgs is, in fact, a
    // []u8. But this ThreadPool is private and being used for 2 specific cases
    // that we control.

    var field_types: [ARG_COUNT]type = undefined;
    inline for (full_fields[0..ARG_COUNT], 0..) |field, i| {
        field_types[i] = field.type;
    }
    return @Tuple(&field_types);
}

const t = @import("t.zig");
test "ThreadPool: batch add" {
    defer t.reset();

    const counts = [_]u32{ 1, 2, 3, 4, 5, 6 };
    const backlogs = [_]u32{ 1, 2, 3, 4, 5, 6 };
    for (counts) |count| {
        for (backlogs) |backlog| {
            testSum = 0; // global defined near the end of this file
            testCount = 0; // global defined near the end of this file
            testC1 = 0;
            testC2 = 0;
            testC3 = 0;
            testC4 = 0;
            testC5 = 0;
            testC6 = 0;
            var tp = try ThreadPool(testIncr).init(t.io, t.arena.allocator(), .{ .count = count, .backlog = backlog, .buffer_size = 512 });
            defer tp.deinit();

            for (0..1_000) |_| {
                tp.spawn(.{1});
                tp.spawn(.{2});
                tp.spawn(.{3});
                tp.spawn(.{4});
            }
            while (tp.empty() == false) {
                try t.io.sleep(.fromMilliseconds(1), .awake);
            }
            tp.stop();
            try t.expectEqual(10_000, testSum);
            try t.expectEqual(4_000, testCount);

            try t.expectEqual(1000, testC1);
            try t.expectEqual(1000, testC2);
            try t.expectEqual(1000, testC3);
            try t.expectEqual(1000, testC4);
            try t.expectEqual(0, testC5);
            try t.expectEqual(0, testC6);
        }
    }
}

test "ThreadPool: small fuzz" {
    defer t.reset();

    testSum = 0; // global defined near the end of this file
    testCount = 0; // global defined near the end of this file
    testC1 = 0;
    testC2 = 0;
    testC3 = 0;
    testC4 = 0;
    testC5 = 0;
    testC6 = 0;
    var tp = try ThreadPool(testIncr).init(t.io, t.arena.allocator(), .{ .count = 3, .backlog = 3, .buffer_size = 512 });
    defer tp.deinit();

    for (0..10_000) |_| {
        tp.spawn(.{1});
        tp.spawn(.{2});
        tp.spawn(.{3});
    }
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    tp.stop();
    try t.expectEqual(60_000, testSum);
    try t.expectEqual(30_000, testCount);
    try t.expectEqual(10_000, testC1);
    try t.expectEqual(10_000, testC2);
    try t.expectEqual(10_000, testC3);
    try t.expectEqual(0, testC4);
    try t.expectEqual(0, testC5);
    try t.expectEqual(0, testC6);
}

test "ThreadPool: large fuzz" {
    defer t.reset();

    testSum = 0; // global defined near the end of this file
    testCount = 0; // global defined near the end of this file
    testC1 = 0;
    testC2 = 0;
    testC3 = 0;
    testC4 = 0;
    testC5 = 0;
    testC6 = 0;
    var tp = try ThreadPool(testIncr).init(t.io, t.arena.allocator(), .{ .count = 50, .backlog = 1000, .buffer_size = 512 });
    defer tp.deinit();

    for (0..10_000) |_| {
        tp.spawn(.{1});
        tp.spawn(.{2});
        tp.spawn(.{3});
        tp.spawn(.{4});
        tp.spawn(.{5});
        tp.spawn(.{6});
    }
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    tp.stop();
    try t.expectEqual(210_000, testSum);
    try t.expectEqual(60_000, testCount);
    try t.expectEqual(10_000, testC1);
    try t.expectEqual(10_000, testC2);
    try t.expectEqual(10_000, testC3);
    try t.expectEqual(10_000, testC4);
    try t.expectEqual(10_000, testC5);
    try t.expectEqual(10_000, testC6);
}

test "ThreadPool: parked thread cannot starve queued jobs" {
    // Regression for the shared injector queue (CHANGES.md Patch 2): with the
    // old per-thread private queues + round-robin dispatch, the parked
    // thread's queue black-holed its share of these jobs and this test would
    // never reach testCount == 100.
    defer t.reset();

    testCount = 0;
    testParkActive.store(false, .monotonic);
    testParkGate.store(true, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 2, .backlog = 8, .buffer_size = 512 });
    defer tp.deinit();

    tp.spawnOne(.{PARK_JOB});
    while (testParkActive.load(.monotonic) == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    for (0..100) |_| {
        tp.spawnOne(.{1});
    }
    while (@atomicLoad(u64, &testCount, .monotonic) < 100) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    testParkGate.store(false, .monotonic);
    tp.stop();
    try t.expectEqual(100, testCount);
}

test "ThreadPool: pending reports queued depth" {
    defer t.reset();

    testCount = 0;
    testParkActive.store(false, .monotonic);
    testParkGate.store(true, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 1, .backlog = 8, .buffer_size = 512 });
    defer tp.deinit();

    tp.spawnOne(.{PARK_JOB});
    while (testParkActive.load(.monotonic) == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    // the single thread is parked, so these three can only sit in the queue
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});
    try t.expectEqual(3, tp.pending());

    testParkGate.store(false, .monotonic);
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    tp.stop();
    try t.expectEqual(0, tp.pending());
    try t.expectEqual(3, testCount);
}

test "ThreadPool: stop drains jobs queued before it" {
    // Pins the drain-before-exit semantic (CHANGES.md Patch 2): getNext checks
    // queue-empty BEFORE stopped, so jobs queued before stop() are claimed and
    // executed before the pool threads exit. If getNext were reordered to
    // honor stopped first, the three queued jobs below would be dropped and
    // testCount would stay 0.
    defer t.reset();

    testCount = 0;
    testParkActive.store(false, .monotonic);
    testParkGate.store(true, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 1, .backlog = 8, .buffer_size = 512 });
    defer tp.deinit();

    tp.spawnOne(.{PARK_JOB});
    while (testParkActive.load(.monotonic) == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    // the single thread is parked, so these three can only sit in the queue
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});

    // release the parked thread and stop immediately (no waiting): stop()
    // must join only after the already-queued jobs were claimed and executed
    testParkGate.store(false, .monotonic);
    tp.stop();
    try t.expectEqual(3, testCount);
}

test "ThreadPool: stop is idempotent" {
    defer t.reset();

    testCount = 0;
    testParkGate.store(false, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 2, .backlog = 8, .buffer_size = 512 });
    defer tp.deinit();

    tp.spawnOne(.{1});
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    tp.stop();
    // second call must return without crashing or double-joining the threads
    tp.stop();
    try t.expectEqual(1, testCount);
}

test "ThreadPool: pending counts across ring wraparound" {
    // Pins the head < tail branch of pending() (queue.len - tail + head).
    // backlog = 4 -> ring of 4 slots, one kept open -> capacity 3. The park
    // job advances head and tail to 1; the three queued jobs walk head
    // 1 -> 2 -> 3 -> 0 (wraps), so head(0) < tail(1) when pending() runs.
    defer t.reset();

    testCount = 0;
    testParkActive.store(false, .monotonic);
    testParkGate.store(true, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 1, .backlog = 4, .buffer_size = 512 });
    defer tp.deinit();

    tp.spawnOne(.{PARK_JOB});
    while (testParkActive.load(.monotonic) == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }

    // the single thread is parked, so these three can only sit in the queue
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});
    tp.spawnOne(.{1});
    try t.expectEqual(3, tp.pending());

    testParkGate.store(false, .monotonic);
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    tp.stop();
    try t.expectEqual(0, tp.pending());
    try t.expectEqual(3, testCount);
}

test "ThreadPool: batch push wakes enough threads for the batch" {
    // Pins the ready.len == 1 -> signal else broadcast choice in push(): a
    // single push of a two-job batch needs two idle threads woken. With a
    // signal-only mutation one thread wakes and parks in job 1 while job 2
    // sits queued forever; the parked count never reaches 2 and the bounded
    // poll below fails the test instead of hanging.
    defer t.reset();

    testCount = 0;
    testParkActive.store(false, .monotonic);
    testParkActiveCount.store(0, .monotonic);
    testParkGate.store(true, .monotonic);

    var tp = try ThreadPool(testParkOrIncr).init(t.io, t.arena.allocator(), .{ .count = 2, .backlog = 8, .buffer_size = 512 });
    defer tp.deinit();

    // one push() call with a two-element slice: spawn only stages into the
    // producer batch, flush(2) hands batch[0..2] to push in a single call
    tp.spawn(.{PARK_JOB});
    tp.spawn(.{PARK_JOB});
    tp.flush(2);

    var elapsed_ms: usize = 0;
    while (testParkActiveCount.load(.monotonic) < 2 and elapsed_ms < 2000) : (elapsed_ms += 1) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    try t.expectEqual(2, testParkActiveCount.load(.monotonic));

    testParkGate.store(false, .monotonic);
    while (tp.empty() == false) {
        try t.io.sleep(.fromMilliseconds(1), .awake);
    }
    tp.stop();
}

var testSum: u64 = 0;
var testCount: u64 = 0;
var testC1: u64 = 0;
var testC2: u64 = 0;
var testC3: u64 = 0;
var testC4: u64 = 0;
var testC5: u64 = 0;
var testC6: u64 = 0;
fn testIncr(c: u64, buf: []u8) void {
    std.debug.assert(buf.len == 512);
    _ = @atomicRmw(u64, &testSum, .Add, c, .monotonic);
    _ = @atomicRmw(u64, &testCount, .Add, 1, .monotonic);
    switch (c) {
        1 => _ = @atomicRmw(u64, &testC1, .Add, 1, .monotonic),
        2 => _ = @atomicRmw(u64, &testC2, .Add, 1, .monotonic),
        3 => _ = @atomicRmw(u64, &testC3, .Add, 1, .monotonic),
        4 => _ = @atomicRmw(u64, &testC4, .Add, 1, .monotonic),
        5 => _ = @atomicRmw(u64, &testC5, .Add, 1, .monotonic),
        6 => _ = @atomicRmw(u64, &testC6, .Add, 1, .monotonic),
        else => unreachable,
    }
    // let the threadpool queue get backed up
    t.io.sleep(.fromMicroseconds(20), .awake) catch unreachable;
}

const PARK_JOB: u64 = 999;
var testParkGate = std.atomic.Value(bool).init(false);
var testParkActive = std.atomic.Value(bool).init(false);
// concurrently-parked thread count; tests that read it reset it themselves
var testParkActiveCount = std.atomic.Value(u32).init(0);
fn testParkOrIncr(c: u64, buf: []u8) void {
    _ = buf;
    if (c == PARK_JOB) {
        testParkActive.store(true, .monotonic);
        _ = testParkActiveCount.fetchAdd(1, .monotonic);
        while (testParkGate.load(.monotonic)) {
            t.io.sleep(.fromMilliseconds(1), .awake) catch unreachable;
        }
        return;
    }
    _ = @atomicRmw(u64, &testCount, .Add, 1, .monotonic);
}
