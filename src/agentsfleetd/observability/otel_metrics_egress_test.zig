//! Egress-removal guards: the pull endpoint's route identity, the rendering
//! layer, and the deployment scrape blocks are gone; the store-side liveness
//! rule covers the observability hole their removal opens; and the other two
//! signals still export after the metric widening. The census/coverage half
//! lives in otel_metrics_census_test.zig. Registered from tests.zig.
//!
//! Needles for the removed surfaces are spelled as comptime concatenations so
//! the repository-wide orphan greps (the quoted pull path, the renderer entry
//! point) can never match this guard's own source.

const std = @import("std");
const common = @import("common");
const otel_logs = @import("otel_logs.zig");
const otel_traces = @import("otel_traces.zig");
const trace = @import("trace.zig");
const window = @import("otel_metrics_window_test.zig");

const METRICS_PATH_NEEDLE = "\"/met" ++ "rics\"";
const RENDER_ENTRY_NEEDLE = "render" ++ "Prometheus";
const SCRAPE_BLOCK_NEEDLE = "[[met" ++ "rics]]";
const ALERTS_JSON_PATH = "playbooks/operations/observability/providers/grafana/assets/alerts.json";
const ALERTS_SH_PATH = "playbooks/operations/observability/providers/grafana/alerts.sh";
const FLY_TOML_PATHS = [_][]const u8{
    "deploy/fly/agentsfleetd-dev/fly.toml",
    "deploy/fly/agentsfleetd-prod/fly.toml",
};
const MAX_FILE_BYTES = 1024 * 1024;
const LIVENESS_RULE_NAME = "metrics-exporter-dead";
const SATURATION_FAMILY = "agentsfleet_api_in_flight_requests"; // pin test: literal is the contract
const SERVICE_LABEL_ATTACHMENT = "service:\"agentsfleetd\""; // pin test: literal is the contract — alerts.sh label block

fn readRepoFile(alloc: std.mem.Allocator, path: []const u8) !?[]u8 {
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(MAX_FILE_BYTES)) catch |err| switch (err) {
        error.FileNotFound => null,
        else => err,
    };
}

// ── Dimension 4.2 — the route identity is absent from every surface ─────────

test "test_metrics_route_identity_is_absent" {
    const ROUTE_SOURCES = [_]struct { name: []const u8, text: []const u8 }{
        .{ .name = "routes.zig", .text = @embedFile("../http/routes.zig") },
        .{ .name = "router.zig", .text = @embedFile("../http/router.zig") },
        .{ .name = "route_template.zig", .text = @embedFile("../http/route_template.zig") },
        .{ .name = "route_scopes.zig", .text = @embedFile("../http/route_scopes.zig") },
        .{ .name = "route_table.zig", .text = @embedFile("../http/route_table.zig") },
        .{ .name = "route_trace.zig", .text = @embedFile("../http/route_trace.zig") },
    };
    for (ROUTE_SOURCES) |source| {
        if (std.mem.indexOf(u8, source.text, METRICS_PATH_NEEDLE) != null) {
            std.debug.print("FAIL: {s} still carries the removed metrics path literal\n", .{source.name});
            return error.RouteIdentitySurvives;
        }
    }
}

// ── Dimension 4.3 — no rendering entry point survives ───────────────────────

test "test_no_prometheus_rendering_entry_point_remains" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var src_dir = try std.Io.Dir.cwd().openDir(io, "src/agentsfleetd", .{ .iterate = true });
    defer src_dir.close(io);
    var walker = try src_dir.walk(alloc);
    defer walker.deinit();
    var scanned: usize = 0;
    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, ".zig")) continue;
        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(MAX_FILE_BYTES));
        defer alloc.free(content);
        scanned += 1;
        if (std.mem.indexOf(u8, content, RENDER_ENTRY_NEEDLE) != null) {
            std.debug.print("FAIL: {s} still carries a rendering entry point\n", .{entry.basename});
            return error.RenderingEntryPointSurvives;
        }
    }
    try std.testing.expect(scanned > 0); // an empty walk would pass vacuously
}

// ── Dimension 4.4 — no deployment configuration declares a scrape block ─────

test "test_deploy_configs_declare_no_scrape_block" {
    const alloc = std.testing.allocator;
    for (FLY_TOML_PATHS) |path| {
        const toml = (try readRepoFile(alloc, path)) orelse return error.SkipZigTest;
        defer alloc.free(toml);
        if (std.mem.indexOf(u8, toml, SCRAPE_BLOCK_NEEDLE) != null) {
            std.debug.print("FAIL: {s} still declares a scrape block\n", .{path});
            return error.ScrapeBlockSurvives;
        }
    }
}

// ── Dimension 6.1 — the liveness rule is absence-based ──────────────────────

test "test_liveness_rule_fires_on_absent_series" {
    const alloc = std.testing.allocator;
    const alerts_text = (try readRepoFile(alloc, ALERTS_JSON_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(alerts_text);

    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, alerts_text, .{});
    defer parsed.deinit();

    const rules = parsed.value.array;
    for (rules.items) |rule| {
        const name = rule.object.get("name").?.string;
        if (!std.mem.eql(u8, name, LIVENESS_RULE_NAME)) continue;
        const expr = rule.object.get("expr").?.string;
        // The rule evaluates ABSENCE in the metric store: absent() yields no
        // series while the family reports (a present series does not fire it)
        // and exactly one when it stops — no threshold over the value at all.
        try std.testing.expect(std.mem.startsWith(u8, expr, "absent("));
        try std.testing.expect(std.mem.indexOf(u8, expr, SATURATION_FAMILY) != null);
        try std.testing.expect(std.mem.indexOfScalar(u8, expr, '>') == null);
        try std.testing.expect(std.mem.indexOfScalar(u8, expr, '<') == null);
        // A dead datasource must page too, not report "no data, all fine".
        try std.testing.expectEqualStrings("Alerting", rule.object.get("noDataState").?.string);
        try std.testing.expectEqualStrings("Alerting", rule.object.get("execErrState").?.string);
        return;
    }
    return error.LivenessRuleMissing;
}

// ── Dimension 6.2 — the asset validates and carries the service label ───────

test "test_liveness_rule_validates_and_carries_service_label" {
    const alloc = std.testing.allocator;
    const alerts_text = (try readRepoFile(alloc, ALERTS_JSON_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(alerts_text);

    // The asset validates: it parses, and every rule carries the fields the
    // provisioning script consumes.
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, alerts_text, .{});
    defer parsed.deinit();
    const rules = parsed.value.array;
    try std.testing.expect(rules.items.len > 0);
    for (rules.items) |rule| {
        for ([_][]const u8{ "name", "title", "severity", "for", "noDataState", "execErrState", "expr", "summary" }) |field| {
            if (rule.object.get(field) == null) {
                std.debug.print("FAIL: alert rule is missing `{s}`\n", .{field});
                return error.AlertRuleFieldMissing;
            }
        }
    }

    // The service label the routing policy matches is attached by the
    // provisioning script to every rule, the liveness rule included.
    const script = (try readRepoFile(alloc, ALERTS_SH_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(script);
    try std.testing.expect(std.mem.indexOf(u8, script, SERVICE_LABEL_ATTACHMENT) != null);
}

// ── Regression — traces and logs are unaffected by the metric widening ──────

test "test_traces_and_logs_unaffected_by_metric_widening" {
    const alloc = std.testing.allocator;

    // Logs: enqueue one record, collect, and the resourceLogs envelope still
    // parses and carries the record.
    otel_logs.testSetInstalled(window.TEST_CFG);
    defer otel_logs.testClear();
    otel_logs.enqueue("info", "census", "signal isolation probe");
    const logs_body = (try otel_logs.testCollect(alloc, window.TEST_CFG)) orelse return error.NoLogsBody;
    defer alloc.free(logs_body);
    try std.testing.expect(std.mem.indexOf(u8, logs_body, "\"resourceLogs\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, logs_body, "signal isolation probe") != null);
    const logs_parsed = try std.json.parseFromSlice(std.json.Value, alloc, logs_body, .{});
    logs_parsed.deinit();

    // Traces: enqueue one span, collect, and the resourceSpans envelope still
    // parses and carries the span.
    otel_traces.testSetInstalled(window.TEST_CFG);
    defer otel_traces.testClear();
    const ctx = trace.TraceContext.generate();
    const span = otel_traces.buildSpan(ctx, "census.probe", .internal, 100, 200);
    otel_traces.enqueueSpan(span);
    const traces_body = (try otel_traces.testCollect(alloc, window.TEST_CFG)) orelse return error.NoTracesBody;
    defer alloc.free(traces_body);
    try std.testing.expect(std.mem.indexOf(u8, traces_body, "\"resourceSpans\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, traces_body, "\"name\":\"census.probe\"") != null);
    const traces_parsed = try std.json.parseFromSlice(std.json.Value, alloc, traces_body, .{});
    traces_parsed.deinit();

    // And the widened metrics window itself still serializes beside them.
    const metrics_body = try window.flushWindowJson(alloc);
    defer alloc.free(metrics_body);
    try std.testing.expect(std.mem.indexOf(u8, metrics_body, "\"resourceMetrics\"") != null);
}
