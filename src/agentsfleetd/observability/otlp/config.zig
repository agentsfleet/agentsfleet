//! Grafana Cloud OTLP config shared by all three exporters (traces/logs/metrics).
//! Moved out of otel_logs.zig so the exporters depend on a neutral config module
//! rather than on the logs signal. Same env gate as before:
//! GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY.

const std = @import("std");
const common = @import("common");
const env_resolve = @import("../../config/env_resolve.zig");

const EnvMap = common.env.Map;

pub const GrafanaOtlpConfig = struct {
    endpoint: []const u8,
    instance_id: []const u8,
    api_key: []const u8,
    service_name: []const u8 = "agentsfleetd",
};

/// Try to load Grafana OTLP config from environment. Returns null when not configured.
pub fn configFromEnv(env_map: *const EnvMap, alloc: std.mem.Allocator) ?GrafanaOtlpConfig {
    const endpoint = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_ENDPOINT") orelse return null;
    const trimmed = std.mem.trim(u8, endpoint, " \t\r\n");
    if (trimmed.len == 0) {
        alloc.free(endpoint);
        return null;
    }
    const instance_id = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_INSTANCE_ID") orelse {
        alloc.free(endpoint);
        return null;
    };
    const api_key = env_resolve.config(env_map, alloc, "GRAFANA_OTLP_API_KEY") orelse {
        alloc.free(endpoint);
        alloc.free(instance_id);
        return null;
    };
    const service_name = env_resolve.config(env_map, alloc, "OTEL_SERVICE_NAME") orelse {
        return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key };
    };
    return .{ .endpoint = endpoint, .instance_id = instance_id, .api_key = api_key, .service_name = service_name };
}
