//! Grafana Cloud OTLP config shared by all three exporters (traces/logs/metrics),
//! plus the one resource + instrumentation-scope serializer they all emit. Same
//! env gate as before: GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID,
//! GRAFANA_OTLP_API_KEY.
//!
//! The resource lives here rather than in any one signal because all three
//! envelopes must carry byte-identical service identity — a log, a span, and a
//! metric from this process describe the same service or they cannot be
//! correlated in Grafana. `semconv.zig` owns the keys; this file owns the
//! values and their lifetime.

const std = @import("std");
const common = @import("common");
const build_options = @import("build_options");
const env_resolve = @import("../../config/env_resolve.zig");
const semconv = @import("../semconv.zig");

const EnvMap = common.env.Map;

const ENV_ENDPOINT = "GRAFANA_OTLP_ENDPOINT";
const ENV_INSTANCE_ID = "GRAFANA_OTLP_INSTANCE_ID";
const ENV_API_KEY = "GRAFANA_OTLP_API_KEY";
const ENV_SERVICE_NAME = "OTEL_SERVICE_NAME";
/// Operator-supplied replica identity. Absent by default: a fabricated instance
/// id would multiply every series by the replica count without being trustworthy.
const ENV_SERVICE_INSTANCE_ID = "OTEL_SERVICE_INSTANCE_ID";
const TRIM_CHARS = " \t\r\n";

pub const GrafanaOtlpConfig = struct {
    endpoint: []const u8,
    instance_id: []const u8,
    api_key: []const u8,
    service_name: []const u8 = semconv.SCOPE_NAME,
    service_namespace: []const u8 = semconv.SERVICE_NAMESPACE,
    service_version: []const u8 = build_options.version,
    /// `service.instance.id` — emitted only when an operator supplied one.
    service_instance_id: ?[]const u8 = null,

    /// Frees every env-owned field. Only `configFromEnv` results may be
    /// passed — hand-built configs with static strings must not call this
    /// (`configFromEnv` dupes `service_name` even when the env override is
    /// absent, precisely so this free is unconditional).
    pub fn deinit(self: *GrafanaOtlpConfig, alloc: std.mem.Allocator) void {
        alloc.free(self.endpoint);
        alloc.free(self.instance_id);
        alloc.free(self.api_key);
        alloc.free(self.service_name);
        if (self.service_instance_id) |v| alloc.free(v);
        self.* = undefined;
    }
};

/// Try to load Grafana OTLP config from environment. Returns null when not configured.
pub fn configFromEnv(env_map: *const EnvMap, alloc: std.mem.Allocator) ?GrafanaOtlpConfig {
    const endpoint = env_resolve.config(env_map, alloc, ENV_ENDPOINT) orelse return null;
    const trimmed = std.mem.trim(u8, endpoint, TRIM_CHARS);
    if (trimmed.len == 0) {
        alloc.free(endpoint);
        return null;
    }
    const instance_id = env_resolve.config(env_map, alloc, ENV_INSTANCE_ID) orelse {
        alloc.free(endpoint);
        return null;
    };
    const api_key = env_resolve.config(env_map, alloc, ENV_API_KEY) orelse {
        alloc.free(endpoint);
        alloc.free(instance_id);
        return null;
    };
    // service_name is ALWAYS owned (the default is duped) so deinit frees it
    // unconditionally; an OOM here degrades to "not configured", matching the
    // `config` env-resolution policy of the three loads above.
    const service_name = env_resolve.config(env_map, alloc, ENV_SERVICE_NAME) orelse
        alloc.dupe(u8, semconv.SCOPE_NAME) catch {
        alloc.free(endpoint);
        alloc.free(instance_id);
        alloc.free(api_key);
        return null;
    };
    var cfg: GrafanaOtlpConfig = .{
        .endpoint = endpoint,
        .instance_id = instance_id,
        .api_key = api_key,
        .service_name = service_name,
    };
    cfg.service_instance_id = trustedInstanceId(env_map, alloc);
    return cfg;
}

/// An operator-supplied instance id, or null. A blank value is treated as
/// absent so an empty deployment variable cannot put an empty standard
/// attribute on every signal.
fn trustedInstanceId(env_map: *const EnvMap, alloc: std.mem.Allocator) ?[]const u8 {
    const raw = env_resolve.config(env_map, alloc, ENV_SERVICE_INSTANCE_ID) orelse return null;
    if (std.mem.trim(u8, raw, TRIM_CHARS).len == 0) {
        alloc.free(raw);
        return null;
    }
    return raw;
}

/// Serialize the envelope prefix every OTLP signal shares: the standard
/// resource, the instrumentation scope carrying the build version, and the
/// pinned core schema URL — up to and including the opening bracket of the
/// signal's item array. The caller appends items, then closes with
/// `ENVELOPE_SUFFIX`.
///
/// `resource_key`/`scope_key`/`items_key` are the signal's three OTLP-JSON
/// field names (for metrics: `resourceMetrics`, `scopeMetrics`, `metrics`).
pub fn appendEnvelopePrefix(
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    cfg: GrafanaOtlpConfig,
    comptime resource_key: []const u8,
    comptime scope_key: []const u8,
    comptime items_key: []const u8,
) !void {
    try list.print(alloc, "{{\"" ++ resource_key ++ "\":[{{\"resource\":{{\"attributes\":[", .{});
    try appendStringAttr(list, alloc, semconv.RESOURCE_SERVICE_NAME, cfg.service_name, true);
    try appendStringAttr(list, alloc, semconv.RESOURCE_SERVICE_NAMESPACE, cfg.service_namespace, false);
    try appendStringAttr(list, alloc, semconv.RESOURCE_SERVICE_VERSION, cfg.service_version, false);
    if (cfg.service_instance_id) |instance| {
        try appendStringAttr(list, alloc, semconv.RESOURCE_SERVICE_INSTANCE_ID, instance, false);
    }
    try list.print(
        alloc,
        "]}},\"" ++ scope_key ++ "\":[{{\"scope\":{{\"name\":\"{s}\",\"version\":{f}}},\"schemaUrl\":\"{s}\",\"" ++ items_key ++ "\":[",
        .{ semconv.SCOPE_NAME, std.json.fmt(cfg.service_version, .{}), semconv.CORE_SCHEMA_URL },
    );
}

/// Closes what `appendEnvelopePrefix` opened: items array, scope object, scope
/// array, resource object, resource array, envelope.
pub const ENVELOPE_SUFFIX = "]}]}]}";

/// One OTLP `KeyValue` with a string value. Both key and value route through
/// `std.json.fmt`, which supplies the quotes and escapes the interior, so a
/// hostile service name can never break out of the envelope.
fn appendStringAttr(
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    key: []const u8,
    value: []const u8,
    first: bool,
) !void {
    if (!first) try list.appendSlice(alloc, ",");
    try list.print(alloc, "{{\"key\":{f},\"value\":{{\"stringValue\":{f}}}}}", .{
        std.json.fmt(key, .{}),
        std.json.fmt(value, .{}),
    });
}

test {
    _ = @import("config_test.zig");
}
