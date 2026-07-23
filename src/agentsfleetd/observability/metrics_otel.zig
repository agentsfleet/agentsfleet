//! Fixed-cardinality Prometheus state for local OTLP exporter health.

const std = @import("std");

pub const QUEUE_DEPTH_NAME = "agentsfleet_otlp_queue_depth";
pub const QUEUE_DEPTH_HELP = "Current entries buffered for OpenTelemetry Protocol export.";
pub const DISCARDED_NAME = "agentsfleet_otlp_entries_discarded_total";
pub const DISCARDED_HELP = "Entries discarded locally or reported rejected by the OpenTelemetry Protocol backend.";

pub const Signal = enum(u8) {
    logs,
    traces,
    metrics,
};

pub const DiscardReason = enum(u8) {
    ring_full,
    aggregate_cap,
    serialize_failed,
    partial_rejected,
    export_rejected,
    export_uncertain,
};

pub const SIGNALS = [_]Signal{ .logs, .traces, .metrics };
pub const DISCARD_REASONS = [_]DiscardReason{
    .ring_full,
    .aggregate_cap,
    .serialize_failed,
    .partial_rejected,
    .export_rejected,
    .export_uncertain,
};

const SIGNAL_COUNT = SIGNALS.len;
const REASON_COUNT = DISCARD_REASONS.len;
const DISCARD_SERIES_COUNT = SIGNAL_COUNT * REASON_COUNT;

pub const Snapshot = struct {
    queue_depth: [SIGNAL_COUNT]u64,
    discarded: [SIGNAL_COUNT][REASON_COUNT]u64,
};

comptime {
    std.debug.assert(@sizeOf(Snapshot) == 168);
}

var g_queue_depth = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** SIGNAL_COUNT;
var g_discarded = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** DISCARD_SERIES_COUNT;

fn signalIndex(signal: Signal) usize {
    return @intFromEnum(signal);
}

fn discardIndex(signal: Signal, reason: DiscardReason) usize {
    return signalIndex(signal) * REASON_COUNT + @intFromEnum(reason);
}

/// Replace the current bounded queue depth for one fixed signal.
pub fn setQueueDepth(signal: Signal, depth: usize) void {
    g_queue_depth[signalIndex(signal)].store(@intCast(depth), .release);
}

/// Add an exact discard count to one fixed signal and reason.
pub fn recordDiscard(signal: Signal, reason: DiscardReason, count: usize) void {
    if (count == 0) return;
    _ = g_discarded[discardIndex(signal, reason)].fetchAdd(@intCast(count), .monotonic);
}

/// Copy all exporter-health values into one stable rendering snapshot.
pub fn snapshot() Snapshot {
    // SAFETY: the loops below write every queue-depth and discard cell before
    // the value is returned; the fixed signal/reason sets make that exhaustive.
    var result: Snapshot = undefined;
    for (SIGNALS, 0..) |signal, signal_idx| {
        result.queue_depth[signal_idx] = g_queue_depth[signalIndex(signal)].load(.acquire);
        for (DISCARD_REASONS, 0..) |reason, reason_idx| {
            result.discarded[signal_idx][reason_idx] =
                g_discarded[discardIndex(signal, reason)].load(.acquire);
        }
    }
    return result;
}

/// Clear process-global values between deterministic unit tests.
pub fn resetForTest() void {
    for (&g_queue_depth) |*value| value.store(0, .release);
    for (&g_discarded) |*value| value.store(0, .release);
}

test {
    _ = @import("metrics_otel_test.zig");
}
