//! Generic OTLP exporter flush driver.
//!
//! Owns the parts every signal shares: the config, the background flush thread
//! (tick-interruptible so uninstall wakes within one tick — the Listener/Hang
//! shutdown-must-wake rule), the drain-on-shutdown loop, and one persistent HTTP
//! client reused across flushes. Each signal supplies only `collect` (produce the
//! OTLP-JSON body for pending data) + `pending` (is anything buffered?) + `path`.
//!
//! `Exporter(hooks)` returns a type with module-level state, so each signal gets
//! its OWN instantiation (its own ring/registry + thread). This is the single
//! copy of the lifecycle that otel_traces/otel_logs/otel_metrics used to triplicate.

const std = @import("std");
const config = @import("config.zig");
const Client = @import("Client.zig");

const logging = @import("log");

const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;
const OTLP_PAYLOAD_BUF_BYTES: usize = 256 * 1024;

pub const Hooks = struct {
    /// OTLP path, e.g. "/v1/traces".
    path: []const u8,
    /// Log scope for export-failure warns (e.g. `.otel_traces`).
    scope: @TypeOf(.enum_literal),
    /// Produce the OTLP-JSON body for pending data into `alloc` (the envelope
    /// needs `cfg.service_name`), or null if nothing to send this flush. Called
    /// only by the flush thread.
    collect: fn (std.mem.Allocator, config.GrafanaOtlpConfig) anyerror!?[]const u8,
    /// True while data remains buffered (drives the shutdown-drain loop).
    pending: fn () bool,
    flush_interval_ms: u64 = 5_000,
    wake_threshold: u32 = 50,
};

pub fn Exporter(comptime hooks: Hooks) type {
    return struct {
        var g_config: ?config.GrafanaOtlpConfig = null;
        var g_thread: ?std.Thread = null;
        var g_running = std.atomic.Value(bool).init(false);
        var g_io: ?std.Io = null;
        var g_wake_count = std.atomic.Value(u32).init(0);
        var g_event: std.Io.Event = .unset;

        const log = logging.scoped(hooks.scope);

        pub const InstallOutcome = enum { installed, already_running, spawn_failed };

        /// Install the exporter. Starts the background flush thread. The claim
        /// is a single atomic swap — a racing second install loses and changes
        /// nothing (two flush threads on one ring would put two consumers on a
        /// single-consumer buffer, and the overwritten thread handle would
        /// never be joined).
        pub fn install(io: std.Io, cfg: config.GrafanaOtlpConfig) InstallOutcome {
            if (g_running.swap(true, .acq_rel)) return .already_running;
            g_io = io;
            g_config = cfg;
            g_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
                g_config = null;
                g_io = null;
                g_running.store(false, .release);
                return .spawn_failed;
            };
            return .installed;
        }

        /// Stop the flush thread and drain. Wakes the tick sleep within one tick.
        pub fn uninstall() void {
            g_running.store(false, .release);
            if (g_io) |io| g_event.set(io);
            if (g_thread) |t| {
                t.join();
                g_thread = null;
            }
            g_config = null;
            g_io = null;
            g_wake_count.store(0, .release);
            g_event.reset();
        }

        pub fn isInstalled() bool {
            // Gate on the atomic only. g_config is a non-atomic optional that
            // uninstall() nulls on another thread; reading it here (concurrently
            // with a producer's record call) would be a data race. The flush
            // thread re-checks `g_config orelse return` before any use, so
            // g_running is the sufficient and race-free gate.
            return g_running.load(.acquire);
        }

        /// Wake the flush thread once the fixed signal threshold is reached.
        /// The interval timeout remains the liveness bound when traffic is low.
        pub fn notify() void {
            if (!g_running.load(.acquire)) return;
            const count = g_wake_count.fetchAdd(1, .monotonic) + 1;
            if (count >= hooks.wake_threshold) {
                g_wake_count.store(0, .release);
                if (g_io) |io| g_event.set(io);
            }
        }

        fn flushLoop() void {
            const io = g_io orelse return;
            var client = Client.init(io);
            defer client.deinit();
            while (g_running.load(.acquire)) {
                g_event.waitTimeout(io, .{ .duration = .{ .raw = .fromMilliseconds(@intCast(hooks.flush_interval_ms)), .clock = .awake } }) catch |err| switch (err) {
                    error.Timeout => {},
                    error.Canceled => return,
                };
                g_event.reset();
                if (!g_running.load(.acquire)) break;
                flushOnce(&client);
            }
            // Drain on shutdown against one absolute monotonic deadline. No
            // new POST starts after the deadline, even if collection is slow.
            const deadline = std.Io.Clock.boot.now(io).toNanoseconds() + @as(i96, SHUTDOWN_DRAIN_TIMEOUT_MS) * std.time.ns_per_ms;
            while (hooks.pending() and std.Io.Clock.boot.now(io).toNanoseconds() < deadline) {
                flushOnce(&client);
            }
        }

        fn flushOnce(client: *Client) void {
            const cfg = g_config orelse return;
            var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
            var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
            const alloc = fba.allocator();
            const body = (hooks.collect(alloc, cfg) catch return) orelse return;
            _ = client.post(alloc, cfg, hooks.path, body) catch |err|
                log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });
        }

        /// Test hook: mark installed WITHOUT spawning the flush thread, so emit +
        /// buffer inspection is deterministic.
        pub fn testSetInstalled(cfg: config.GrafanaOtlpConfig) void {
            g_config = cfg;
            g_running.store(true, .release);
        }

        /// Test hook: clear installed state (does not touch the signal's buffer).
        pub fn testClear() void {
            g_running.store(false, .release);
            g_config = null;
            g_wake_count.store(0, .release);
            g_event.reset();
        }
    };
}

test {
    _ = @import("exporter_test.zig");
}
