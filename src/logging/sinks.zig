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

pub fn emitToSinks(
    level: std.log.Level,
    scope: []const u8,
    ts_ms: i64,
    body: []const u8,
) void {
    sinks_mutex.lock();
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
        self.buf.deinit(self.alloc);
    }

    pub fn sink(self: *BufferedSink) Sink {
        return .{ .emit = emit, .ctx = @ptrCast(self) };
    }

    pub fn snapshot(self: *BufferedSink) []const u8 {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.buf.items;
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

    const captured = bs.snapshot();
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

    try std.testing.expect(std.mem.indexOf(u8, bs.snapshot(), "event=first") != null);
    try std.testing.expect(std.mem.indexOf(u8, bs.snapshot(), "event=dropped") == null);
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
    var newlines: usize = 0;
    for (bs.snapshot()) |c| {
        if (c == '\n') newlines += 1;
    }
    try std.testing.expectEqual(MAX_SINKS, newlines);
}
