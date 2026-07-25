// Tests for the one resource + instrumentation-scope serializer all three OTLP
// signals share (src/agentsfleetd/observability/otlp/config.zig).
//
// The invariant under test: a log, a span, and a metric leaving this process
// describe the SAME service, byte for byte. If they drift, nothing in Grafana
// correlates them, and the drift is invisible until an operator tries to join
// the three signals during an incident. Nothing at build time couples the three
// call sites, so this is the only thing holding them together.

const std = @import("std");
const config = @import("config.zig");
const semconv = @import("../semconv.zig");

const ALLOC = std.testing.allocator;

/// The three signals differ only in their OTLP-JSON key names; every other byte
/// of the prefix is the shared identity.
const Signal = struct { resource: []const u8, scope: []const u8, items: []const u8 };

fn baseConfig() config.GrafanaOtlpConfig {
    return .{ .endpoint = "https://otlp.example", .instance_id = "1", .api_key = "k" };
}

/// Render one signal's full envelope with an empty item array, so the result is
/// parseable JSON rather than a dangling prefix.
fn renderEnvelope(
    alloc: std.mem.Allocator,
    cfg: config.GrafanaOtlpConfig,
    comptime signal: Signal,
) ![]u8 {
    var list: std.ArrayList(u8) = .empty;
    errdefer list.deinit(alloc);
    try config.appendEnvelopePrefix(&list, alloc, cfg, signal.resource, signal.scope, signal.items);
    try list.appendSlice(alloc, config.ENVELOPE_SUFFIX);
    return list.toOwnedSlice(alloc);
}

/// Pull the `resource` object back out of a rendered envelope, so the three
/// signals are compared on parsed identity rather than on raw substrings.
fn resourceOf(parsed: std.json.Value, comptime resource_key: []const u8) std.json.Value {
    return parsed.object.get(resource_key).?.array.items[0].object.get("resource").?;
}

fn attrValue(resource: std.json.Value, key: []const u8) ?[]const u8 {
    for (resource.object.get("attributes").?.array.items) |attr| {
        if (std.mem.eql(u8, attr.object.get("key").?.string, key)) {
            return attr.object.get("value").?.object.get("stringValue").?.string;
        }
    }
    return null;
}

test "test_otlp_resources_share_semantic_identity" {
    const cfg = baseConfig();

    const logs = try renderEnvelope(ALLOC, cfg, .{ .resource = "resourceLogs", .scope = "scopeLogs", .items = "logRecords" });
    defer ALLOC.free(logs);
    const spans = try renderEnvelope(ALLOC, cfg, .{ .resource = "resourceSpans", .scope = "scopeSpans", .items = "spans" });
    defer ALLOC.free(spans);
    const metrics = try renderEnvelope(ALLOC, cfg, .{ .resource = "resourceMetrics", .scope = "scopeMetrics", .items = "metrics" });
    defer ALLOC.free(metrics);

    var logs_doc = try std.json.parseFromSlice(std.json.Value, ALLOC, logs, .{});
    defer logs_doc.deinit();
    var spans_doc = try std.json.parseFromSlice(std.json.Value, ALLOC, spans, .{});
    defer spans_doc.deinit();
    var metrics_doc = try std.json.parseFromSlice(std.json.Value, ALLOC, metrics, .{});
    defer metrics_doc.deinit();

    const log_resource = resourceOf(logs_doc.value, "resourceLogs");
    const span_resource = resourceOf(spans_doc.value, "resourceSpans");
    const metric_resource = resourceOf(metrics_doc.value, "resourceMetrics");

    // Identity is equal across all three signals, key by key.
    for ([_][]const u8{
        semconv.RESOURCE_SERVICE_NAME,
        semconv.RESOURCE_SERVICE_NAMESPACE,
        semconv.RESOURCE_SERVICE_VERSION,
    }) |key| {
        const from_logs = attrValue(log_resource, key) orelse return error.ResourceKeyMissing;
        try std.testing.expectEqualStrings(from_logs, attrValue(span_resource, key) orelse return error.ResourceKeyMissing);
        try std.testing.expectEqualStrings(from_logs, attrValue(metric_resource, key) orelse return error.ResourceKeyMissing);
    }

    // The namespace is the pinned product value, not whatever the config held.
    try std.testing.expectEqualStrings(
        semconv.SERVICE_NAMESPACE,
        attrValue(log_resource, semconv.RESOURCE_SERVICE_NAMESPACE).?,
    );

    // Every signal carries the pinned CORE schema URL. The pinned GenAI commit
    // publishes none, so no second schema URL may appear beside it.
    for ([_][]const u8{ logs, spans, metrics }) |envelope| {
        try std.testing.expect(std.mem.indexOf(u8, envelope, semconv.CORE_SCHEMA_URL) != null);
        try std.testing.expectEqual(@as(usize, 1), std.mem.count(u8, envelope, "\"schemaUrl\""));
        try std.testing.expect(std.mem.indexOf(u8, envelope, "semantic-conventions-genai") == null);
    }
}

test "an absent instance id puts no service.instance.id on any signal" {
    var cfg = baseConfig();
    cfg.service_instance_id = null;

    const envelope = try renderEnvelope(ALLOC, cfg, .{ .resource = "resourceLogs", .scope = "scopeLogs", .items = "logRecords" });
    defer ALLOC.free(envelope);

    // Absent means the key is gone, NOT present-and-empty: an empty standard
    // attribute on every signal is worse than no attribute, because it looks
    // like a real instance whose id happens to be blank.
    try std.testing.expect(std.mem.indexOf(u8, envelope, semconv.RESOURCE_SERVICE_INSTANCE_ID) == null);

    var doc = try std.json.parseFromSlice(std.json.Value, ALLOC, envelope, .{});
    defer doc.deinit();
    try std.testing.expect(attrValue(resourceOf(doc.value, "resourceLogs"), semconv.RESOURCE_SERVICE_INSTANCE_ID) == null);
}

test "a trusted instance id appears on every signal identically" {
    var cfg = baseConfig();
    cfg.service_instance_id = "replica-7";

    inline for ([_]Signal{
        .{ .resource = "resourceLogs", .scope = "scopeLogs", .items = "logRecords" },
        .{ .resource = "resourceSpans", .scope = "scopeSpans", .items = "spans" },
        .{ .resource = "resourceMetrics", .scope = "scopeMetrics", .items = "metrics" },
    }) |signal| {
        const envelope = try renderEnvelope(ALLOC, cfg, signal);
        defer ALLOC.free(envelope);
        var doc = try std.json.parseFromSlice(std.json.Value, ALLOC, envelope, .{});
        defer doc.deinit();
        try std.testing.expectEqualStrings(
            "replica-7",
            attrValue(resourceOf(doc.value, signal.resource), semconv.RESOURCE_SERVICE_INSTANCE_ID).?,
        );
    }
}

test "a hostile service identity cannot break out of the envelope" {
    var cfg = baseConfig();
    // Quote + backslash + raw newline + a brace run: the shapes that break a
    // hand-rolled serializer. `service_name` and `service_instance_id` are the
    // two operator-supplied values, so both get the hostile treatment.
    cfg.service_name = "evil\",\"key\":\"injected\\\n{}";
    cfg.service_instance_id = "\"}]}],\"resourceLogs\":[";

    const envelope = try renderEnvelope(ALLOC, cfg, .{ .resource = "resourceLogs", .scope = "scopeLogs", .items = "logRecords" });
    defer ALLOC.free(envelope);

    // Still exactly one envelope, and it parses.
    var doc = try std.json.parseFromSlice(std.json.Value, ALLOC, envelope, .{});
    defer doc.deinit();
    try std.testing.expectEqual(@as(usize, 1), doc.value.object.count());

    const resource = resourceOf(doc.value, "resourceLogs");
    // The hostile bytes survive as DATA, not as structure.
    try std.testing.expectEqualStrings(cfg.service_name, attrValue(resource, semconv.RESOURCE_SERVICE_NAME).?);
    try std.testing.expectEqualStrings(cfg.service_instance_id.?, attrValue(resource, semconv.RESOURCE_SERVICE_INSTANCE_ID).?);
    // No injected attribute smuggled itself in beside the real ones.
    try std.testing.expect(attrValue(resource, "key") == null);
}
