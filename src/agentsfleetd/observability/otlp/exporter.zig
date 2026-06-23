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
const common = @import("common");
const clock = common.clock;
const config = @import("config.zig");
const post = @import("post.zig");

const logging = @import("log");

const SHUTDOWN_DRAIN_TIMEOUT_MS: u64 = 5_000;
const SLEEP_TICK_MS: u64 = 100;
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
};

pub fn Exporter(comptime hooks: Hooks) type {
    return struct {
        var g_config: ?config.GrafanaOtlpConfig = null;
        var g_thread: ?std.Thread = null;
        var g_running = std.atomic.Value(bool).init(false);

        const log = logging.scoped(hooks.scope);

        /// Install the exporter. Starts the background flush thread.
        pub fn install(cfg: config.GrafanaOtlpConfig) void {
            g_config = cfg;
            g_running.store(true, .release);
            g_thread = std.Thread.spawn(.{}, flushLoop, .{}) catch {
                g_config = null;
                g_running.store(false, .release);
                return;
            };
        }

        /// Stop the flush thread and drain. Wakes the tick sleep within one tick.
        pub fn uninstall() void {
            g_running.store(false, .release);
            if (g_thread) |t| {
                t.join();
                g_thread = null;
            }
            g_config = null;
        }

        pub fn isInstalled() bool {
            return g_running.load(.acquire) and g_config != null;
        }

        fn interruptibleSleep(total_ms: u64) void {
            var slept: u64 = 0;
            while (slept < total_ms and g_running.load(.acquire)) : (slept += SLEEP_TICK_MS) {
                common.sleepNanos(SLEEP_TICK_MS * std.time.ns_per_ms);
            }
        }

        fn flushLoop() void {
            var client = post.Client.init();
            defer client.deinit();
            while (g_running.load(.acquire)) {
                interruptibleSleep(hooks.flush_interval_ms);
                flushOnce(&client);
            }
            // Drain on shutdown.
            const deadline = clock.nowMillis() + @as(i64, @intCast(SHUTDOWN_DRAIN_TIMEOUT_MS));
            while (hooks.pending() and clock.nowMillis() < deadline) {
                flushOnce(&client);
            }
        }

        fn flushOnce(client: *post.Client) void {
            const cfg = g_config orelse return;
            var payload_buf: [OTLP_PAYLOAD_BUF_BYTES]u8 = undefined;
            var fba = std.heap.FixedBufferAllocator.init(&payload_buf);
            const alloc = fba.allocator();
            const body = (hooks.collect(alloc, cfg) catch return) orelse return;
            client.post(alloc, cfg, hooks.path, body) catch |err|
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
        }
    };
}

test {
    _ = @import("exporter_test.zig");
}
