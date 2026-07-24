//! Log sink registry (LOGGING_STANDARD §13).
//!
//! `agentsfleetdLog` formats each record once (level/scope/ts_ms/body) and
//! fans out to every registered `Sink`. Two production sinks register
//! at boot — one renders + writes to stderr, one enqueues to the OTLP
//! exporter. Tests install a `BufferedSink` to assert on emitted lines
//! without subprocess capture.
//!
//! Until the first `registerSink` call, `emitToSinks` is a no-op so
//! `agentsfleetdLog` callers (which fire from comptime-plugged
//! `std_options.logFn` as early as `applyEnvSources` in main) can fall
//! back to `fatalStderr` or direct write without dropping observable
//! lines. The `sinksRegistered` predicate exists for exactly that
//! pre-init fallback in `agentsfleetdLog`.
//!
//! No runtime growth — `MAX_SINKS = 4` covers prod (stderr + OTLP)
//! plus 2 test slots (additive capture + spare). Registration is
//! mutex-protected; the emit fan-out snapshots the array under the
//! lock then releases before invoking sinks so a slow OTLP enqueue
//! cannot block log emit on a different thread.
//!
//! Safety properties:
//!   • `snapshot` returns OWNED bytes (caller frees with bs.alloc) —
//!     a live-slice return raced with concurrent ArrayList realloc.
//!   • `deinit` unregisters, snapshots the started-ticket counter,
//!     then waits until completed catches up — bounded to the pre-
//!     removal in-flight set, not the global emit load.

const std = @import("std");
const common = @import("common");

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
var sinks_mutex: common.Mutex = .{};
// Epoch-split emit tickets. Each emit joins the CURRENT epoch under
// sinks_mutex at snapshot time and completes into that same epoch.
// unregisterByCtx flips the epoch after compaction and waits for the
// OLD epoch to drain — post-removal emits land in the new epoch, so
// their completions can never satisfy the pre-removal drain (a single
// started/completed pair let exactly that happen: fast post-removal
// completions covered for a still-running pre-removal emit).
var emit_epoch: usize = 0; // guarded by sinks_mutex (flips under unregister_mutex + sinks_mutex)
var emit_started: [2]std.atomic.Value(u64) = .{ .{ .raw = 0 }, .{ .raw = 0 } };
var emit_completed: [2]std.atomic.Value(u64) = .{ .{ .raw = 0 }, .{ .raw = 0 } };
// Serializes unregister drains: an epoch flip while another drain is
// in flight would recycle the epoch that drain still counts on.
var unregister_mutex: common.Mutex = .{};

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

pub fn clearSinksForTest() void {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    sinks_len = 0;
}

pub fn sinksRegistered() bool {
    sinks_mutex.lock();
    defer sinks_mutex.unlock();
    return sinks_len > 0;
}

/// Remove every entry whose `ctx` matches AND drain any concurrent
/// `emitToSinks` that snapshotted the registry before our removal. Used
/// by `BufferedSink.deinit` so a single bs.deinit() (1) pulls all the
/// caller's registrations out and (2) blocks until any prior snapshot
/// has finished fan-out, making the ctx safe to free. Public so any
/// stateful sink owner (and the drain-property test) can use the same
/// safe removal path.
pub fn unregisterByCtx(ctx: *const anyopaque) void {
    unregister_mutex.lock();
    defer unregister_mutex.unlock();
    sinks_mutex.lock();
    var write_idx: usize = 0;
    for (sinks_buf[0..sinks_len]) |s| {
        if (@as(*const anyopaque, s.ctx) != ctx) {
            sinks_buf[write_idx] = s;
            write_idx += 1;
        }
    }
    sinks_len = write_idx;
    // Flip the epoch while still holding sinks_mutex: every emit that
    // snapshotted the registry pre-compaction sits in the OLD epoch;
    // emits after the unlock join the new one and cannot mask a
    // still-running pre-removal fan-out.
    const old = emit_epoch;
    emit_epoch ^= 1;
    const drain_target = emit_started[old].load(.acquire);
    sinks_mutex.unlock();
    while (emit_completed[old].load(.acquire) < drain_target) std.atomic.spinLoopHint();
    // Old epoch fully drained — recycle its counters for the next flip
    // (safe: epoch flips serialize on unregister_mutex, and no live
    // emit can still hold the old epoch index).
    emit_started[old].store(0, .release);
    emit_completed[old].store(0, .release);
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
    // lock is mandatory: a clearSinksForTest racing with `n = sinks_len`
    // would otherwise see a stale non-zero length.
    if (sinks_len == 0) {
        sinks_mutex.unlock();
        return;
    }
    var snapshot_arr: [MAX_SINKS]Sink = undefined;
    const n = sinks_len;
    for (sinks_buf[0..n], 0..) |s, i| snapshot_arr[i] = s;
    // Join the current epoch INSIDE the lock — atomic with the registry
    // snapshot, so unregisterByCtx's flip cleanly partitions emits into
    // pre-removal (old epoch, drained) and post-removal (new epoch).
    const epoch = emit_epoch;
    _ = emit_started[epoch].fetchAdd(1, .acq_rel);
    sinks_mutex.unlock();
    defer _ = emit_completed[epoch].fetchAdd(1, .release);
    for (snapshot_arr[0..n]) |s| s.emit(s.ctx, level, scope, ts_ms, body);
}

/// Test-only sink that appends every emitted body to a heap buffer.
/// One newline appended per emit so `std.mem.indexOf` searches across
/// multi-emit captures cleanly. Thread-safe; install via
/// `registerSink(bs.sink())` and drain via `snapshot()`.
pub const BufferedSink = struct {
    alloc: std.mem.Allocator,
    buf: std.ArrayList(u8) = .empty,
    /// Guards `buf`; `snapshot()` copies under it so a concurrent emit can't realloc mid-read.
    mutex: common.Mutex = .{},

    pub fn init(alloc: std.mem.Allocator) BufferedSink {
        return .{ .alloc = alloc };
    }

    pub fn deinit(self: *BufferedSink) void {
        // unregisterByCtx (1) removes our entries from the registry so
        // no future emit targets us AND (2) waits for the pre-removal
        // in-flight emits to complete before returning. After it
        // returns, no thread holds a dangling pointer to self — safe
        // to free the backing buffer.
        unregisterByCtx(@ptrCast(self));
        self.buf.deinit(self.alloc);
        self.* = undefined;
    }

    pub fn sink(self: *BufferedSink) Sink {
        return .{ .emit = emit, .ctx = @ptrCast(self) };
    }

    /// Return an owned copy of the current buffer contents. Caller
    /// frees with the BufferedSink's allocator (`bs.alloc`). The copy
    /// is taken under lock so concurrent `emit`s can't invalidate the
    /// returned slice via ArrayList realloc — the previous
    /// `return self.buf.items` was a race waiting to happen. Caller must free.
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

/// Test-only window into the emit ticket counters (summed across both
/// epochs). Used by sinks_test.zig to pin the bounded-drain property
/// after compaction; production callers have no use for these and
/// should not introduce one.
pub fn emitTicketsForTest() struct { started: u64, completed: u64 } {
    return .{
        .started = emit_started[0].load(.acquire) + emit_started[1].load(.acquire),
        .completed = emit_completed[0].load(.acquire) + emit_completed[1].load(.acquire),
    };
}

// Tests live in sinks_test.zig — kept here so this source file stays
// under the 350-line cap. The reference lives inside sinks.zig (not
// main.zig) so the test file lands in the `log` module alongside
// sinks.zig and can `@import("sinks.zig")` directly without crossing
// a module boundary.
test {
    _ = @import("sinks_test.zig");
}
