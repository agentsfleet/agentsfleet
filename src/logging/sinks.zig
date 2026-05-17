//! Log sink registry (LOGGING_STANDARD §13).
//!
//! `zombiedLog` formats each record once (level/scope/ts_ms/body) and
//! fans out to every registered `Sink`. Two production sinks register
//! at boot — one renders + writes to stderr, one enqueues to the OTLP
//! exporter. Tests install a `BufferedSink` to assert on emitted lines
//! without subprocess capture.
//!
//! Until the first `registerSink` call, `emitToSinks` is a no-op so
//! `zombiedLog` callers (which fire from comptime-plugged
//! `std_options.logFn` as early as `applyEnvSources` in main) can fall
//! back to `fatalStderr` or direct write without dropping observable
//! lines. The `sinksRegistered` predicate exists for exactly that
//! pre-init fallback in `zombiedLog`.
//!
//! No runtime growth — `MAX_SINKS = 4` covers prod (stderr + OTLP)
//! plus 2 test slots (additive capture + spare). Registration is
//! mutex-protected; the emit fan-out snapshots the array under the
//! lock then releases before invoking sinks so a slow OTLP enqueue
//! cannot block log emit on a different thread.
//!
//! Safety properties:
//!   • `snapshot` returns an OWNED copy — caller frees with bs.alloc.
//!     A live-slice return raced with concurrent emits' ArrayList
//!     realloc, turning indexOf into use-after-free.
//!   • `deinit` self-unregisters then drains its per-sink mutex, so
//!     the API is safe against defer-ordering bugs that would leave
//!     a stack-freed ctx in the global registry.

const std = @import("std");

/// Sink fn signature. Sinks receive the post-fmt body (logfmt body, no
/// envelope) plus level/scope/ts_ms so each sink owns its own format
/// choice — stderr sink renders pretty/logfmt envelope, OTLP sink
/// forwards body verbatim, BufferedSink appends body to a heap buffer.
pub const SinkEmit = *const fn (
    ctx: *anyopaque,
    level: std.log.Level,
    scope: []const u8,
    ts_ms: i64,
    body: []const u8,
) void;

pub const Sink = struct {
    emit: SinkEmit,
    /// Sink-private state. Sinks with no state pass a sentinel pointer
    /// (`&stateless_marker`); the emit fn must not dereference it.
    ctx: *anyopaque,
};

const MAX_SINKS: usize = 4;

var sinks_buf: [MAX_SINKS]Sink = undefined;
var sinks_len: usize = 0;
var sinks_mutex: std.Thread.Mutex = .{};

/// Sentinel pointer for stateless sinks (stderr, OTLP). Never read by
/// the emit fn — just satisfies the `*anyopaque` non-null contract.
var stateless_marker: u8 = 0;
pub fn statelessCtx() *anyopaque {
    return @ptrCast(&stateless_marker);
}

pub fn registerSink(sink: Sink) void {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    if (sinks_len >= MAX_SINKS) return;
    sinks_buf[sinks_len] = sink;
    sinks_len += 1;
}

pub fn clearSinks() void {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    sinks_len = 0;
}

pub fn sinksRegistered() bool {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    return sinks_len > 0;
}

// Remove every entry whose `ctx` matches. Used by `BufferedSink.deinit`
// so a single bs.deinit() pulls all of its registrations out of the
// registry, regardless of how many times the same sink was registered.
// Compacts in place so other sinks keep their positions.
fn unregisterByCtx(ctx: *const anyopaque) void {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    var write_idx: usize = 0;
    for (sinks_buf[0..sinks_len]) |s| {
        if (@as(*const anyopaque, s.ctx) != ctx) {
            sinks_buf[write_idx] = s;
            write_idx += 1;
        }
    }
    sinks_len = write_idx;
}

pub fn emitToSinks(
    level: std.log.Level,
    scope: []const u8,
    ts_ms: i64,
    body: []const u8,
) void {
    sinks_mutex.lock();
    // Early-return under lock — when no sinks are registered, the test
    // mode redirect (mod.zig) still calls into here on every emit, so
    // we short-circuit before allocating the snapshot array. Under
    // lock is mandatory: a clearSinks racing with `n = sinks_len`
    // would otherwise see a stale non-zero length.
    if (sinks_len == 0) {
        sinks_mutex.unlock();
        return;
    }
    var snapshot_arr: [MAX_SINKS]Sink = undefined;
    const n = sinks_len;
    for (sinks_buf[0..n], 0..) |s, i| snapshot_arr[i] = s;
    sinks_mutex.unlock();
    for (snapshot_arr[0..n]) |s| s.emit(s.ctx, level, scope, ts_ms, body);
}

/// Test-only sink that appends every emitted body to a heap buffer.
/// One newline appended per emit so `std.mem.indexOf` searches across
/// multi-emit captures cleanly. Thread-safe; install via
/// `registerSink(bs.sink())` and drain via `snapshot()`.
pub const BufferedSink = struct {
    alloc: std.mem.Allocator,
    buf: std.ArrayList(u8) = .{},
    mutex: std.Thread.Mutex = .{},

    pub fn init(alloc: std.mem.Allocator) BufferedSink {
        return .{ .alloc = alloc };
    }

    pub fn deinit(self: *BufferedSink) void {
        // Step 1: pull ourselves out of the global registry so no NEW
        // emit can target this BufferedSink. Without this, defer
        // ordering bugs (`defer bs.deinit()` declared before
        // `defer clearSinks()` runs bs.deinit FIRST per LIFO) leave a
        // dangling stack-pointer ctx in the registry that the next
        // emit dereferences.
        unregisterByCtx(@ptrCast(self));
        // Step 2: drain any in-flight `BufferedSink.emit` call by
        // taking the per-sink mutex. After unregisterByCtx no new
        // emits target self, but emit calls already past their
        // snapshot in emitToSinks may still be inside BufferedSink.emit
        // (which takes self.mutex). Acquiring + releasing here is a
        // serialization point — after we own the lock once, all
        // in-flight emits have completed and it's safe to free.
        self.mutex.lock();
        self.mutex.unlock();
        self.buf.deinit(self.alloc);
    }

    pub fn sink(self: *BufferedSink) Sink {
        return .{ .emit = emit, .ctx = @ptrCast(self) };
    }

    /// Return an owned copy of the current buffer contents. Caller
    /// frees with the BufferedSink's allocator (`bs.alloc`). The copy
    /// is taken under lock so concurrent `emit`s can't invalidate the
    /// returned slice via ArrayList realloc — the previous
    /// `return self.buf.items` was a race waiting to happen.
    pub fn snapshot(self: *BufferedSink) ![]u8 {
        self.mutex.lock();
        defer self.mutex.unlock();
        return try self.alloc.dupe(u8, self.buf.items);
    }

    fn emit(
        ctx: *anyopaque,
        level: std.log.Level,
        scope: []const u8,
        ts_ms: i64,
        body: []const u8,
    ) void {
        _ = level;
        _ = scope;
        _ = ts_ms;
        const self: *BufferedSink = @ptrCast(@alignCast(ctx));
        self.mutex.lock();
        defer self.mutex.unlock();
        self.buf.appendSlice(self.alloc, body) catch return;
        self.buf.append(self.alloc, '\n') catch return;
    }
};

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
