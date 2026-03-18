//! Result emission — structured logging, JSON stdout, and OTLP push.
//!
//! Exports:
//!   - `emitResult`      — emit structured log line, JSON to stdout, OTLP push,
//!                         and update the daemon-state dead-letter counter.
//!   - `pushOtelMetrics` — best-effort OTLP metrics export.
//!
//! Ownership:
//!   - `emitResult` borrows all parameters; it does not retain any slice after
//!     returning.
//!   - `pushOtelMetrics` allocates and frees OTLP config internally via defer.

const std = @import("std");
const outbox = @import("../../state/outbox_reconciler.zig");
const otel = @import("../../observability/otel_export.zig");
const state_mod = @import("state.zig");

const log = std.log.scoped(.reconcile);

pub fn emitResult(
    alloc: std.mem.Allocator,
    start_ms: i64,
    result: ?outbox.ReconcileResult,
    err: ?anyerror,
) void {
    const elapsed_ms = std.time.milliTimestamp() - start_ms;
    const dead_lettered = if (result) |r| r.dead_lettered else 0;
    const status: []const u8 = if (err != null) "error" else "ok";
    const err_name: []const u8 = if (err) |e| @errorName(e) else "none";

    // Structured log line — always emitted, picked up by any log aggregator.
    log.info(
        "reconcile_result status={s} dead_lettered={d} elapsed_ms={d} error={s}",
        .{ status, dead_lettered, elapsed_ms, err_name },
    );

    // Structured JSON to stdout for machine parsing (cron output capture, CloudWatch, etc.)
    var stdout_buf: [512]u8 = undefined;
    var stdout_writer = std.fs.File.stdout().writer(&stdout_buf);
    const json = std.fmt.bufPrint(
        &stdout_buf,
        "{{\"event\":\"reconcile\",\"status\":\"{s}\",\"dead_lettered\":{d},\"elapsed_ms\":{d},\"error\":\"{s}\"}}\n",
        .{ status, dead_lettered, elapsed_ms, err_name },
    ) catch return;
    stdout_writer.interface.writeAll(json) catch {};
    stdout_writer.interface.flush() catch {};

    // OTLP push if configured — fire-and-forget.
    pushOtelMetrics(alloc, dead_lettered);

    if (state_mod.g_daemon_state) |s| {
        s.last_dead_lettered.store(dead_lettered, .release);
    }
}

pub fn pushOtelMetrics(alloc: std.mem.Allocator, dead_lettered: u32) void {
    const cfg = otel.configFromEnv(alloc) orelse return;
    defer {
        alloc.free(cfg.endpoint);
        if (!std.mem.eql(u8, cfg.service_name, "zombied")) {
            alloc.free(cfg.service_name);
        }
    }

    // The metrics snapshot includes the outbox dead-letter counter we just incremented.
    otel.exportMetricsSnapshotBestEffort(alloc, cfg, false, null, null);
    log.info("otel_push_attempted endpoint={s} dead_lettered={d}", .{
        cfg.endpoint,
        dead_lettered,
    });
}
