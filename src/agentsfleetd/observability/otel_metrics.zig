//! OTLP JSON metric exporter for Grafana Cloud Mimir.
//! Ring buffer + background flush thread. The metering hot paths push samples
//! (credit-drain sum, token-throughput sum, run-latency histogram); this module
//! batches and POSTs to GRAFANA_OTLP_ENDPOINT/v1/metrics.
//!
//! Fire-and-forget: export errors increment counters / log a warn, never block
//! callers. Config + auth are reused verbatim from otel_logs.zig (the same
//! GRAFANA_OTLP_* gate that enables traces and logs). DELTA temporality — see
//! otel_metrics_payload.zig.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const otel_logs = @import("otel_logs.zig");
const payload = @import("otel_metrics_payload.zig");
const cardinality = @import("otel_metrics_cardinality.zig");

const OTLP_METRICS_PATH = "/v1/metrics";
const BUFFER_CAPACITY: usize = 1024;
const FLUSH_INTERVAL_MS: u64 = 5_000;
const FLUSH_BATCH_SIZE: usize = 50;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;
// Flush-thread sleep is split into short ticks so uninstall() wakes the wait
// within one tick instead of a full flush interval — bounds shutdown latency
// (the Listener/Hang shutdown-must-wake rule) and keeps tests fast.
const SLEEP_TICK_MS: u64 = 100;

const logging = @import("log");
const log = logging.scoped(.otel_metrics);

const Sample = payload.Sample;

// ---------------------------------------------------------------------------
// Ring buffer (lock-free SPMC; identical discipline to otel_traces.zig).
// ---------------------------------------------------------------------------

const Ring = struct {
    const Self = @This();

    // SAFETY: each slot is written by exactly one claiming producer before that
    // slot's ready flag is published; pop reads only ready slots.
    buffer: [BUFFER_CAPACITY]Sample = undefined,
    ready: [BUFFER_CAPACITY]std.atomic.Value(u8) = [_]std.atomic.Value(u8){std.atomic.Value(u8).init(0)} ** BUFFER_CAPACITY,
    head: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    tail: std.atomic.Value(usize) = std.atomic.Value(usize).init(0),
    dropped: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    pub fn push(self: *Self, entry: Sample) bool {
        while (true) {
            const head = self.head.load(.acquire);
            const tail = self.tail.load(.acquire);
            const next_head = (head + 1) % BUFFER_CAPACITY;
            if (next_head == tail) {
                // safe because: independent statistic; no ordering required.
                _ = self.dropped.fetchAdd(1, .monotonic);
                return false;
            }
            // safe because: the .acq_rel cmpxchg claims slot `head` exclusively —
            // a losing producer observes the new head and retries, so two
            // producers can never write the same slot. Failure order .acquire
            // re-reads a coherent head.
            if (self.head.cmpxchgWeak(head, next_head, .acq_rel, .acquire)) |_| continue;
            self.buffer[head] = entry;
            // safe because: .release publishes the completed slot write to
            // pop()'s .acquire load of the same flag.
            self.ready[head].store(1, .release);
            return true;
        }
    }

    pub fn pop(self: *Self) ?Sample {
        // safe because: tail is consumer-owned (single flush thread); head's
        // .acquire pairs with producers' claim cmpxchg.
        const tail = self.tail.load(.acquire);
        const head = self.head.load(.acquire);
        if (head == tail) return null;
        // safe because: .acquire pairs with the producer's ready .release store.
        // A claimed-but-unwritten head-of-line slot reads 0 → treat as empty for
        // this pass; the next flush pass retries (bound: the producer's memcpy).
        if (self.ready[tail].load(.acquire) != 1) return null;
        const entry = self.buffer[tail];
        // safe because: ready clears before the tail .release store, and a
        // producer can only claim this slot after observing the advanced tail —
        // so a fresh claimant always starts from ready == 0.
        self.ready[tail].store(0, .release);
        self.tail.store((tail + 1) % BUFFER_CAPACITY, .release);
        return entry;
    }

    pub fn len(self: *Self) usize {
        // safe because: monotonic-quality snapshot for batching/drain heuristics
        // only; .acquire keeps it no staler than the callers' own loads.
        const head = self.head.load(.acquire);
        const tail = self.tail.load(.acquire);
        if (head >= tail) return head - tail;
        return BUFFER_CAPACITY - tail + head;
    }
};

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

var g_ring: Ring = .{};
var g_config: ?otel_logs.GrafanaOtlpConfig = null;
var g_flush_thread: ?std.Thread = null;
var g_running = std.atomic.Value(bool).init(false);

/// Install the async metric exporter. Starts a background flush thread.
pub fn install(cfg: otel_logs.GrafanaOtlpConfig) void {
    g_config = cfg;
    g_running.store(true, .release);
    g_flush_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
        g_config = null;
        g_running.store(false, .release);
        return;
    };
}

/// Stop the background flush thread and drain remaining samples.
pub fn uninstall() void {
    g_running.store(false, .release);
    if (g_flush_thread) |t| {
        t.join();
        g_flush_thread = null;
    }
    g_config = null;
}

/// Returns true if the async metric exporter is running.
pub fn isInstalled() bool {
    return g_running.load(.acquire) and g_config != null;
}

fn currentNanos() u64 {
    return @intCast(clock.nowNanos());
}

// ---------------------------------------------------------------------------
// Record API — non-blocking, fire-and-forget. No-ops when not installed.
// Callers invoke these AFTER the money transaction commits.
// ---------------------------------------------------------------------------

/// Record a committed credit-drain delta (nanos) labelled by posture/model and,
/// when under the cardinality cap, workspace.
pub fn recordCreditDrain(drained_nanos: i64, posture: []const u8, model: []const u8, workspace: []const u8) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.credit_drain, drained_nanos, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    if (workspace.len > 0 and cardinality.allowWorkspace(workspace)) {
        _ = payload.addLabel(&s, payload.LABEL_WORKSPACE, workspace);
    }
    _ = g_ring.push(s);
}

/// Record a token-throughput delta for one direction (input/cached/output).
pub fn recordTokens(count: i64, direction: []const u8, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.tokens, count, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_DIRECTION, direction);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    _ = g_ring.push(s);
}

/// Observe a run's wall-clock duration (ms) into the latency histogram.
pub fn observeRunDuration(wall_ms: i64, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.run_duration, wall_ms, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    _ = g_ring.push(s);
}

// ---------------------------------------------------------------------------
// Background flush
// ---------------------------------------------------------------------------

fn interruptibleSleep(total_ms: u64) void {
    var slept: u64 = 0;
    while (slept < total_ms and g_running.load(.acquire)) : (slept += SLEEP_TICK_MS) {
        common.sleepNanos(SLEEP_TICK_MS * std.time.ns_per_ms);
    }
}

fn flushLoop() void {
    while (g_running.load(.acquire)) {
        interruptibleSleep(FLUSH_INTERVAL_MS);
        flushBatch();
    }
    // Drain on shutdown.
    const deadline = clock.nowMillis() + @as(i64, @intCast(SHUTDOWN_DRAIN_TIMEOUT_MS));
    while (g_ring.len() > 0 and clock.nowMillis() < deadline) {
        flushBatch();
    }
}

fn flushBatch() void {
    const cfg = g_config orelse return;

    var batch: [FLUSH_BATCH_SIZE]Sample = undefined;
    var n: usize = 0;
    while (n < FLUSH_BATCH_SIZE) {
        batch[n] = g_ring.pop() orelse break;
        n += 1;
    }
    if (n == 0) return;

    const OTLP_PAYLOAD_BUF_BYTES = 256 * 1024;
    var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
    const alloc = fba.allocator();

    const body = payload.serializeBatch(alloc, cfg.service_name, batch[0..n]) catch return;
    otel_logs.postWithBasicAuth(alloc, cfg, OTLP_METRICS_PATH, body) catch |err| log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
}

pub const TestRing = Ring;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

/// Test hook: number of samples currently pending in the global ring.
pub fn testPendingCount() usize {
    return g_ring.len();
}

test {
    _ = @import("otel_metrics_test.zig");
}
