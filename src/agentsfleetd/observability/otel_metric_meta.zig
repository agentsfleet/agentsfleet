//! Metric metadata primitives shared by the closed family registry.

const semconv = @import("semconv.zig");

pub const MetricKind = enum { sum, histogram, gauge };

/// OpenTelemetry Protocol aggregation temporality for sums.
pub const Temporality = enum { delta, cumulative };

/// Unit conversion applied at serialization. Observations stay integer in
/// their source unit; conversion happens only at the wire boundary.
pub const Scale = enum { none, millis_to_seconds, nanos_to_seconds };

pub const MetricMeta = struct {
    name: []const u8,
    unit: []const u8,
    kind: MetricKind,
    monotonic: bool = false,
    temporality: Temporality = .delta,
    bounds: []const u64 = &.{},
    scale: Scale = .none,
    max_series: usize = 1,
    streamed: bool = false,
    cost: bool = false,
    evented: bool = false,
    live_read: bool = false,
};

pub fn cumulative(name: []const u8) MetricMeta {
    return .{ .name = name, .unit = semconv.UNIT_COUNT, .kind = .sum, .monotonic = true, .temporality = .cumulative };
}

pub fn gauge(name: []const u8, unit: []const u8) MetricMeta {
    return .{ .name = name, .unit = unit, .kind = .gauge };
}

pub fn cumulativeBytes(name: []const u8) MetricMeta {
    return .{ .name = name, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative };
}

pub fn liveRead(base: MetricMeta) MetricMeta {
    var meta = base;
    meta.live_read = true;
    return meta;
}

pub fn streamed(base: MetricMeta, worst_case: usize) MetricMeta {
    var meta = base;
    meta.streamed = true;
    meta.max_series = worst_case;
    return meta;
}

pub fn cost(base: MetricMeta) MetricMeta {
    var meta = base;
    meta.cost = true;
    meta.evented = true;
    return meta;
}

pub fn evented(base: MetricMeta) MetricMeta {
    var meta = base;
    meta.evented = true;
    return meta;
}
