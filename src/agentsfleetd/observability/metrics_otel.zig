//! Fixed-cardinality Prometheus state for local OTLP exporter health, including
//! the attribute omissions the metric exporter takes rather than exporting an
//! unbounded, truncated, or non-standard attribute value. Every dimension here
//! is a closed compile-time set: no caller-provided label ever reaches it.

const std = @import("std");
const semconv = @import("semconv.zig");

pub const QUEUE_DEPTH_NAME = "agentsfleet_otlp_queue_depth";
pub const QUEUE_DEPTH_HELP = "Current entries buffered for OpenTelemetry Protocol export.";
pub const DISCARDED_NAME = "agentsfleet_otlp_entries_discarded_total";
pub const DISCARDED_HELP = "Entries discarded locally or reported rejected by the OpenTelemetry Protocol backend.";
pub const ATTRIBUTE_OMITTED_NAME = "agentsfleet_otel_attribute_omitted_total";
pub const ATTRIBUTE_OMITTED_HELP = "Metric attributes omitted to keep series bounded and standard; the measurement itself was still exported.";

/// The only two metric attributes this process may decline to emit. Both are
/// optional in the registry, so omitting one preserves a valid data point.
pub const OmittedAttribute = enum(u8) {
    provider_name,
    request_model,

    /// The wire attribute key, so an operator reads the dashboard label as the
    /// same string the payload would have carried.
    pub fn label(self: OmittedAttribute) []const u8 {
        return switch (self) {
            .provider_name => semconv.ATTR_PROVIDER_NAME,
            .request_model => semconv.ATTR_REQUEST_MODEL,
        };
    }
};

pub const OmissionReason = enum(u8) {
    /// The configured provider has no exact well-known name; inventing one
    /// would publish a private spelling as though it were standard.
    unmapped_provider,
    /// A new (provider, model) pair would push the flush window past its
    /// distinct-series ceiling.
    budget_exhausted,
    /// The value exceeds the fixed payload bound; truncating it would export a
    /// different model or provider under a plausible-looking name.
    value_too_long,

    pub fn label(self: OmissionReason) []const u8 {
        return @tagName(self);
    }
};

pub const OMITTED_ATTRIBUTES = [_]OmittedAttribute{ .provider_name, .request_model };
pub const OMISSION_REASONS = [_]OmissionReason{ .unmapped_provider, .budget_exhausted, .value_too_long };

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
const ATTRIBUTE_COUNT = OMITTED_ATTRIBUTES.len;
const OMISSION_REASON_COUNT = OMISSION_REASONS.len;
const OMISSION_SERIES_COUNT = ATTRIBUTE_COUNT * OMISSION_REASON_COUNT;

pub const Snapshot = struct {
    queue_depth: [SIGNAL_COUNT]u64,
    discarded: [SIGNAL_COUNT][REASON_COUNT]u64,
    attribute_omitted: [ATTRIBUTE_COUNT][OMISSION_REASON_COUNT]u64,
};

comptime {
    std.debug.assert(@sizeOf(Snapshot) == 216);
}

var g_queue_depth = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** SIGNAL_COUNT;
var g_discarded = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** DISCARD_SERIES_COUNT;
var g_attribute_omitted = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** OMISSION_SERIES_COUNT;

fn signalIndex(signal: Signal) usize {
    return @intFromEnum(signal);
}

fn discardIndex(signal: Signal, reason: DiscardReason) usize {
    return signalIndex(signal) * REASON_COUNT + @intFromEnum(reason);
}

fn omissionIndex(attribute: OmittedAttribute, reason: OmissionReason) usize {
    return @as(usize, @intFromEnum(attribute)) * OMISSION_REASON_COUNT + @intFromEnum(reason);
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

/// Count one metric attribute this process declined to emit. The measurement
/// itself is always still exported — this counter is what makes the resulting
/// gap in model or provider attribution visible instead of silent.
pub fn recordAttributeOmission(attribute: OmittedAttribute, reason: OmissionReason) void {
    _ = g_attribute_omitted[omissionIndex(attribute, reason)].fetchAdd(1, .monotonic);
}

/// Copy all exporter-health values into one stable rendering snapshot.
pub fn snapshot() Snapshot {
    // SAFETY: the loops below write every queue-depth, discard, and omission
    // cell before the value is returned; the fixed signal/reason/attribute sets
    // make that exhaustive.
    var result: Snapshot = undefined;
    for (SIGNALS, 0..) |signal, signal_idx| {
        result.queue_depth[signal_idx] = g_queue_depth[signalIndex(signal)].load(.acquire);
        for (DISCARD_REASONS, 0..) |reason, reason_idx| {
            result.discarded[signal_idx][reason_idx] =
                g_discarded[discardIndex(signal, reason)].load(.acquire);
        }
    }
    for (OMITTED_ATTRIBUTES, 0..) |attribute, attribute_idx| {
        for (OMISSION_REASONS, 0..) |reason, reason_idx| {
            result.attribute_omitted[attribute_idx][reason_idx] =
                g_attribute_omitted[omissionIndex(attribute, reason)].load(.acquire);
        }
    }
    return result;
}

/// Clear process-global values between deterministic unit tests.
pub fn resetForTest() void {
    for (&g_queue_depth) |*value| value.store(0, .release);
    for (&g_discarded) |*value| value.store(0, .release);
    for (&g_attribute_omitted) |*value| value.store(0, .release);
}

test {
    _ = @import("metrics_otel_test.zig");
}
