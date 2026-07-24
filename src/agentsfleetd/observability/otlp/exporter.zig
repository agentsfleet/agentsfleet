//! Generic OpenTelemetry Protocol (OTLP) exporter flush driver.
//!
//! Owns the parts every signal shares: the config, the background flush thread
//! (tick-interruptible so uninstall wakes immediately), bounded shutdown drain,
//! and one persistent HTTP client reused across flushes. Each signal supplies
//! its bounded collection and pending-count hooks.
//!
//! `Exporter(hooks)` returns a type with module-level state, so each signal gets
//! its OWN instantiation (its own ring/registry + thread). This is the single
//! copy of the lifecycle that otel_traces/otel_logs/otel_metrics used to triplicate.

const std = @import("std");
const config = @import("config.zig");
const Client = @import("Client.zig");
const health = @import("../metrics_otel.zig");

const logging = @import("log");

const EVENT_EXPORT_FAILED = "otel_export_failed";
const EVENT_INVALID_PARTIAL = "otel_partial_count_invalid";
const EVENT_SERIALIZE_FAILED = "otel_serialize_failed";
const NORMAL_EXPORT_TIMEOUT_MS: u64 = 5_000;
const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;
const OTLP_PAYLOAD_BUF_BYTES: usize = 256 * 1024;

pub const CollectedBatch = struct {
    body: []const u8,
    removed_count: usize,
    export_count: usize,
};

pub const CollectResult = union(enum) {
    empty,
    ready: CollectedBatch,
    serialize_failed: usize,
};

pub const Hooks = struct {
    /// Fixed signal owning this exporter instance.
    signal: health.Signal,
    /// OpenTelemetry Protocol path, e.g. "/v1/traces".
    path: []const u8,
    /// Log scope for export-failure warnings.
    scope: @TypeOf(.enum_literal),
    /// Remove and serialize at most `max_entries` on the sole consumer.
    collect: fn (std.mem.Allocator, config.GrafanaOtlpConfig, usize) CollectResult,
    /// Current bounded queue depth.
    pending_count: fn () usize,
    /// One-shot post seam used for deterministic unit failure injection.
    post: fn (*Client, std.mem.Allocator, config.GrafanaOtlpConfig, []const u8, []const u8, i96) anyerror!Client.ExportResult = Client.post,
    flush_interval_ms: u64 = 5_000,
    wake_threshold: u32 = 50,
    transport_timeout_ms: u64 = NORMAL_EXPORT_TIMEOUT_MS,
    shutdown_timeout_ms: u64 = SHUTDOWN_DRAIN_TIMEOUT_MS,
};

pub fn Exporter(comptime hooks: Hooks) type {
    return struct {
        const RunState = enum(u8) { stopped, starting, running, stopping };

        var g_config: ?config.GrafanaOtlpConfig = null;
        var g_thread: ?std.Thread = null;
        var g_state = std.atomic.Value(RunState).init(.stopped);
        var g_io: ?std.Io = null;
        var g_accepted_since_cycle = std.atomic.Value(u32).init(0);
        var g_shutdown_deadline_ns = std.atomic.Value(i64).init(0);
        var g_event: std.Io.Event = .unset;

        const log = logging.scoped(hooks.scope);

        pub const InstallOutcome = enum { installed, already_running, spawn_failed };

        /// Install the exporter and start its sole consumer thread.
        pub fn install(io: std.Io, cfg: config.GrafanaOtlpConfig) InstallOutcome {
            if (g_state.cmpxchgStrong(.stopped, .starting, .acq_rel, .acquire) != null) {
                return .already_running;
            }
            g_io = io;
            g_config = cfg;
            g_accepted_since_cycle.store(0, .release);
            g_shutdown_deadline_ns.store(0, .release);
            g_event.reset();
            g_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
                g_config = null;
                g_io = null;
                g_state.store(.stopped, .release);
                return .spawn_failed;
            };
            g_state.store(.running, .release);
            return .installed;
        }

        /// Store one shutdown deadline, wake the consumer, drain, and join.
        pub fn uninstall() void {
            if (g_state.load(.acquire) != .running) return;
            if (g_io) |io| {
                g_shutdown_deadline_ns.store(deadlineAfter(io, hooks.shutdown_timeout_ms), .release);
                g_state.store(.stopping, .release);
                g_event.set(io);
            } else {
                g_state.store(.stopping, .release);
            }
            if (g_thread) |t| {
                t.join();
                g_thread = null;
            }
            g_config = null;
            g_io = null;
            g_accepted_since_cycle.store(0, .release);
            g_shutdown_deadline_ns.store(0, .release);
            g_event.reset();
            g_state.store(.stopped, .release);
        }

        pub fn isInstalled() bool {
            return g_state.load(.acquire) == .running;
        }

        /// Count one accepted push and coalesce threshold wakeups per cycle.
        pub fn notifyAccepted() void {
            if (!isInstalled()) return;
            const count = g_accepted_since_cycle.fetchAdd(1, .monotonic) + 1;
            if (count == hooks.wake_threshold) {
                if (g_io) |io| g_event.set(io);
            }
        }

        fn flushLoop() void {
            while (g_state.load(.acquire) == .starting) std.atomic.spinLoopHint();
            if (g_state.load(.acquire) == .stopped) return;
            const io = g_io orelse return;
            var client = Client.init(io);
            defer client.deinit();
            while (g_state.load(.acquire) == .running) {
                g_event.waitTimeout(io, .{ .duration = .{ .raw = .fromMilliseconds(@intCast(hooks.flush_interval_ms)), .clock = .awake } }) catch |err| switch (err) {
                    error.Timeout => {},
                    error.Canceled => return,
                };
                g_event.reset();
                _ = g_accepted_since_cycle.swap(0, .acq_rel);
                if (g_state.load(.acquire) != .running) break;
                flushCycle(&client);
            }
            _ = g_accepted_since_cycle.swap(0, .acq_rel);
            flushCycle(&client);
        }

        fn flushCycle(client: *Client) void {
            const cfg = g_config orelse return;
            var remaining = hooks.pending_count();
            health.setQueueDepth(hooks.signal, remaining);
            while (remaining > 0) {
                const io = g_io orelse return;
                const deadline_ns = postDeadline(io);
                if (deadlineReached(io, deadline_ns)) return;
                var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
                var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
                const alloc = fba.allocator();
                const result = hooks.collect(alloc, cfg, remaining);
                const progressed = handleCollect(&remaining, client, alloc, cfg, result, deadline_ns);
                health.setQueueDepth(hooks.signal, hooks.pending_count());
                if (!progressed) return;
            }
        }

        fn handleCollect(
            remaining: *usize,
            client: *Client,
            alloc: std.mem.Allocator,
            cfg: config.GrafanaOtlpConfig,
            result: CollectResult,
            deadline_ns: i64,
        ) bool {
            return switch (result) {
                .empty => false,
                .serialize_failed => |removed| handleSerializeFailure(remaining, removed),
                .ready => |batch| handleReady(remaining, client, alloc, cfg, batch, deadline_ns),
            };
        }

        fn handleSerializeFailure(remaining: *usize, removed: usize) bool {
            if (removed == 0) return false;
            const bounded = @min(removed, remaining.*);
            remaining.* -= bounded;
            health.recordDiscard(hooks.signal, .serialize_failed, removed);
            log.warn(EVENT_SERIALIZE_FAILED, .{ .count = removed });
            return removed <= bounded;
        }

        fn handleReady(
            remaining: *usize,
            client: *Client,
            alloc: std.mem.Allocator,
            cfg: config.GrafanaOtlpConfig,
            batch: CollectedBatch,
            deadline_ns: i64,
        ) bool {
            if (batch.removed_count == 0 or batch.removed_count > remaining.* or batch.export_count == 0) {
                health.recordDiscard(hooks.signal, .export_uncertain, batch.removed_count);
                return false;
            }
            remaining.* -= batch.removed_count;
            const io = g_io orelse return false;
            if (deadlineReached(io, deadline_ns)) {
                health.recordDiscard(hooks.signal, .export_uncertain, batch.removed_count);
                return false;
            }
            const outcome = hooks.post(
                client,
                alloc,
                cfg,
                hooks.path,
                batch.body,
                deadline_ns,
            );
            recordPostOutcome(outcome, batch);
            return true;
        }

        fn recordPostOutcome(outcome: anyerror!Client.ExportResult, batch: CollectedBatch) void {
            if (outcome) |result| switch (result) {
                .accepted => {},
                .partial_rejected => |rejected| {
                    if (rejected > batch.export_count) {
                        health.recordDiscard(hooks.signal, .export_uncertain, batch.removed_count);
                        log.warn(EVENT_INVALID_PARTIAL, .{
                            .rejected = rejected,
                            .attempted = batch.export_count,
                        });
                    } else {
                        health.recordDiscard(hooks.signal, .partial_rejected, @intCast(rejected));
                    }
                },
            } else |err| {
                const reason: health.DiscardReason =
                    if (err == error.OtlpExportRejected) .export_rejected else .export_uncertain;
                health.recordDiscard(hooks.signal, reason, batch.removed_count);
                log.warn(EVENT_EXPORT_FAILED, .{
                    .reason = @tagName(reason),
                    .err = @errorName(err),
                });
            }
        }

        fn postDeadline(io: std.Io) i64 {
            const normal = deadlineAfter(io, hooks.transport_timeout_ms);
            if (g_state.load(.acquire) != .stopping) return normal;
            const shutdown = g_shutdown_deadline_ns.load(.acquire);
            return @min(normal, shutdown);
        }

        fn deadlineAfter(io: std.Io, timeout_ms: u64) i64 {
            const now: i64 = @intCast(std.Io.Clock.boot.now(io).toNanoseconds());
            const duration: i64 = @intCast(timeout_ms *| std.time.ns_per_ms);
            return now +| duration;
        }

        fn deadlineReached(io: std.Io, deadline_ns: i64) bool {
            return std.Io.Clock.boot.now(io).toNanoseconds() >= deadline_ns;
        }

        /// Test hook: mark installed without spawning the consumer.
        pub fn testSetInstalled(io: std.Io, cfg: config.GrafanaOtlpConfig) void {
            g_io = io;
            g_config = cfg;
            g_accepted_since_cycle.store(0, .release);
            g_state.store(.running, .release);
        }

        /// Test hook: run one bounded cycle synchronously.
        pub fn testFlushCycle() void {
            const io = g_io orelse return;
            var client = Client.init(io);
            defer client.deinit();
            _ = g_accepted_since_cycle.swap(0, .acq_rel);
            flushCycle(&client);
        }

        /// Test hook: read accepted pushes since the last cycle.
        pub fn testAcceptedSinceCycle() u32 {
            return g_accepted_since_cycle.load(.acquire);
        }

        /// Test hook: read and clear the coalesced wake, so a caller can tell a
        /// fresh threshold crossing from pushes that folded into a pending wake.
        pub fn testTakeWakeSignal() bool {
            const signaled = g_event.isSet();
            g_event.reset();
            return signaled;
        }

        /// Test hook: clear installed state without touching the signal buffer.
        pub fn testClear() void {
            g_state.store(.stopped, .release);
            g_config = null;
            g_io = null;
            g_accepted_since_cycle.store(0, .release);
            g_shutdown_deadline_ns.store(0, .release);
            g_event.reset();
        }
    };
}

test {
    _ = @import("exporter_test.zig");
}
