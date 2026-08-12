//! Test-support observation window over the OTLP metrics payload.
//!
//! The retired Prometheus renderer was the shared surface metric tests
//! asserted against; this module rebuilds that window on the exported wire
//! format, so tests keep their no-internals discipline and assert against
//! the path production actually uses. Tests drive a module's public record
//! API, call `flushWindowJson`, and assert on the serialized OTLP-JSON body.
//!
//! Test-only: imported by `*_test.zig` files and registered in tests.zig.
//! Nothing here reaches into instrumentation internals — the window is
//! produced by the exporter's own flush collect hook.

const std = @import("std");
const otel_metrics = @import("otel_metrics.zig");
const otlp_config = @import("otlp/config.zig");

/// Minimal exporter fixture, mirroring otel_metrics_test.zig's config.
pub const TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "test-instance",
    .api_key = "test-key",
    .service_version = "0.0.0-test",
};

/// A serialized metric object opens with its name key; the next occurrence
/// marks the next object, so one family's slice is the span between them.
const NAME_KEY_PREFIX = "\"name\":\"";
/// Wide enough for the prefix, the longest family name, and the close quote.
const NEEDLE_BUF_LEN: usize = 160;

/// Install the exporter test hook, run one flush collect, and return the
/// serialized OTLP-JSON body. Caller frees. The hook is cleared before
/// returning so no installed state leaks across tests.
pub fn flushWindowJson(alloc: std.mem.Allocator) ![]const u8 {
    otel_metrics.testSetInstalled(TEST_CFG);
    defer otel_metrics.testClear();
    return (try otel_metrics.testCollectOnce(alloc, TEST_CFG)) orelse error.EmptyWindow;
}

fn nameNeedle(buf: []u8, family_name: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, NAME_KEY_PREFIX ++ "{s}\"", .{family_name});
}

const ObjectSpan = struct { start: usize, end: usize };

/// The next serialized metric object of `family_name` at or after `from`,
/// or null. The object slice runs to the next metric object's name key.
fn nextFamilyObject(body: []const u8, needle: []const u8, from: usize) ?ObjectSpan {
    const start = std.mem.indexOfPos(u8, body, from, needle) orelse return null;
    const tail_from = start + needle.len;
    const end = std.mem.indexOfPos(u8, body, tail_from, NAME_KEY_PREFIX) orelse body.len;
    return .{ .start = start, .end = end };
}

fn objectHasAll(object: []const u8, fragments: []const []const u8) bool {
    for (fragments) |fragment| {
        if (std.mem.indexOf(u8, object, fragment) == null) return false;
    }
    return true;
}

/// Count serialized objects of `family_name` whose slice carries every
/// fragment. An empty fragment list counts every object of the family.
pub fn countFamilyWith(body: []const u8, family_name: []const u8, fragments: []const []const u8) !usize {
    var needle_buf: [NEEDLE_BUF_LEN]u8 = undefined;
    const needle = try nameNeedle(&needle_buf, family_name);
    var count: usize = 0;
    var from: usize = 0;
    while (nextFamilyObject(body, needle, from)) |span| : (from = span.end) {
        if (objectHasAll(body[span.start..span.end], fragments)) count += 1;
    }
    return count;
}

/// Assert at least one serialized sample of `family_name` is in the window.
pub fn expectFamilySample(body: []const u8, family_name: []const u8) !void {
    try expectFamilyWith(body, family_name, &.{});
}

/// Labeled/valued variant: assert some object of the family carries every
/// fragment (label pairs from `attrFragment`, point values from `intValue`).
pub fn expectFamilyWith(body: []const u8, family_name: []const u8, fragments: []const []const u8) !void {
    if (try countFamilyWith(body, family_name, fragments) > 0) return;
    std.debug.print("FAIL: no `{s}` object carries all {d} fragment(s)\n", .{ family_name, fragments.len });
    return error.FamilySampleMissing;
}

/// Negative variant: no object of the family carries every fragment. With an
/// empty fragment list this asserts the family is absent from the window.
pub fn expectNoFamilyWith(body: []const u8, family_name: []const u8, fragments: []const []const u8) !void {
    if (try countFamilyWith(body, family_name, fragments) == 0) return;
    std.debug.print("FAIL: `{s}` unexpectedly present with the given fragments\n", .{family_name});
    return error.FamilySampleUnexpected;
}

/// The integer point value of the first object of `family_name` carrying
/// every fragment; error.SeriesNotFound when no object matches.
pub fn familyValueWith(body: []const u8, family_name: []const u8, fragments: []const []const u8) !i64 {
    var needle_buf: [NEEDLE_BUF_LEN]u8 = undefined;
    const needle = try nameNeedle(&needle_buf, family_name);
    var from: usize = 0;
    while (nextFamilyObject(body, needle, from)) |span| : (from = span.end) {
        const object = body[span.start..span.end];
        if (!objectHasAll(object, fragments)) continue;
        const value_start = (std.mem.indexOf(u8, object, AS_INT_PREFIX) orelse return error.NotAnIntegerPoint) + AS_INT_PREFIX.len;
        const value_end = std.mem.indexOfScalarPos(u8, object, value_start, '"') orelse return error.NotAnIntegerPoint;
        return std.fmt.parseInt(i64, object[value_start..value_end], 10);
    }
    return error.SeriesNotFound;
}

const AS_INT_PREFIX = "\"asInt\":\"";

/// Format one serialized attribute pair for use as a fragment.
pub fn attrFragment(buf: []u8, key: []const u8, val: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}\",\"value\":{{\"stringValue\":\"{s}\"}}", .{ ATTR_KEY_PREFIX, key, val });
}

const ATTR_KEY_PREFIX = "\"key\":\"";

/// Format one serialized integer point value for use as a fragment.
pub fn intValue(buf: []u8, value: i64) ![]const u8 {
    return std.fmt.bufPrint(buf, AS_INT_PREFIX ++ "{d}\"", .{value});
}

/// A dataPoint with no attributes — the shape every unlabelled family carries.
pub const NO_ATTRIBUTES = "\"attributes\":[]";

test "test_payload_observation_helper_exposes_a_window" {
    const alloc = std.testing.allocator;
    const mc = @import("metrics_counters.zig");

    // Observe through a module's public record API only, then flush.
    mc.setApiInFlightRequests(5);
    const body = try flushWindowJson(alloc);
    defer alloc.free(body);

    // The window is the exporter's own serialized flush: valid OTLP-JSON
    // carrying the recorded family with its live value and no label.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, body, .{});
    parsed.deinit();
    try expectFamilySample(body, "agentsfleet_api_in_flight_requests"); // pin test: literal is the contract
    try std.testing.expectEqual(
        @as(i64, 5),
        try familyValueWith(body, "agentsfleet_api_in_flight_requests", &.{NO_ATTRIBUTES}),
    );
    mc.setApiInFlightRequests(0);
}
