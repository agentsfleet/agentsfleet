//! Census guards for the closed metric-family registry: the operator assets,
//! the architecture census, and the rebuilt test suite must all agree with
//! `otel_metrics_families.zig` — in both directions. The egress-removal
//! guards (route identity, renderer, deploy config, liveness rule) live in
//! otel_metrics_egress_test.zig. Registered from tests.zig.

const std = @import("std");
const common = @import("common");
const families = @import("otel_metrics_families.zig");
const mrp = @import("metrics_redis_pool.zig");
const window = @import("otel_metrics_window_test.zig");

const ALERTS_PATH = "playbooks/operations/observability/providers/grafana/assets/alerts.json";
const DASHBOARD_PATH = "playbooks/operations/observability/providers/grafana/assets/dashboard.json";
const ARCHITECTURE_DOC_PATH = "docs/architecture/observability.md";
const CENSUS_HEADING = "## Metric family census";
const SECTION_BREAK = "\n## ";
const MAX_ASSET_BYTES = 1024 * 1024;
const FAMILY_TOKEN_PREFIX = "agentsfleet_";

/// Every exported family name, pinned as literals. The bidirectional check
/// against the registry below keeps this table exact, and its literals are
/// what lets `test_rebuilt_suite_covers_every_previously_rendered_family`
/// prove each family name is asserted somewhere in the test tree.
// pin test: literal is the contract
const CENSUS_PIN = [_][]const u8{
    "gen_ai.invoke_agent.duration",
    "agentsfleet.invoke_agent.token.usage",
    "agentsfleet.invoke_agent.cache_read.token.usage",
    "agentsfleet.billing.credit.consumed",
    "agentsfleet.telemetry.samples_dropped",
    "agentsfleet_repair_production_to_queue_seconds",
    "agentsfleet_repair_queue_to_completion_seconds",
    "agentsfleet_api_backpressure_rejections_total",
    "agentsfleet_api_in_flight_requests",
    "agentsfleet_sse_backpressure_rejections_total",
    "agentsfleet_sse_in_flight_streams",
    "agentsfleet_sse_dropped_frames_total",
    "agentsfleet_sse_hub_reconnects_total",
    "agentsfleet_worker_running",
    "agentsfleet_fleet_triggered_total",
    "agentsfleet_http_trace_suppressed_total",
    "agentsfleet_otlp_queue_depth",
    "agentsfleet_otlp_entries_discarded_total",
    "agentsfleet_otel_attribute_omitted_total",
    "agentsfleet_signup_bootstrapped_total",
    "agentsfleet_signup_replayed_total",
    "agentsfleet_signup_failed_total",
    "agentsfleet_lease_polls_total",
    "agentsfleet_lease_poll_candidates_scanned_total",
    "agentsfleet_lease_poll_db_roundtrips_total",
    "agentsfleet_fleet_ready_depth",
    "agentsfleet_fleet_ready_write_failures_total",
    "agentsfleet_runner_retention_swept_total",
    "agentsfleet_runner_retention_sweep_failures_total",
    "agentsfleet_account_teardown_unregister_failures_total",
    "agentsfleet_repair_provider_results_total",
    "agentsfleet_repair_correlations_total",
    "agentsfleet_repair_verification_intents_created_total",
    "agentsfleet_repair_dispatch_retried_total",
    "agentsfleet_repair_synthetic_events_total",
    "agentsfleet_repair_verifier_runs_total",
    "agentsfleet_repair_dispatch_due_batch",
    "agentsfleet_repair_dispatch_oldest_age_seconds",
    "agentsfleet_library_stage_duration_seconds_total",
    "agentsfleet_library_stage_observations_total",
    "agentsfleet_library_read_outcome_total",
    "agentsfleet_library_pool_result_total",
    "agentsfleet_library_cache_outcome_total",
    "agentsfleet_library_payload_bytes_total",
    "agentsfleet_library_results_total",
    "agentsfleet_redis_pool_active",
    "agentsfleet_redis_pool_idle",
    "agentsfleet_redis_pool_dials_total",
    "agentsfleet_redis_pool_overflow_dials_total",
    "agentsfleet_redis_pool_poisoned_connections_total",
    "agentsfleet_redis_pool_reconnects_total",
    "agentsfleet_redis_pool_forced_closes_total",
    "agentsfleet_redis_pool_acquire_timeouts_total",
    "agentsfleet_memory_entries_captured_total",
    "agentsfleet_memory_push_failures_total",
    "agentsfleet_memory_hydration_window_entries",
    "agentsfleet_memory_hydration_dropped_entries_total",
    "agentsfleet_memory_hydration_dropped_bytes_total",
    "agentsfleet_memory_cap_evictions_total",
    "agentsfleet_memory_capture_truncated_total",
    "agentsfleet_memory_capture_skipped_total",
    "agentsfleet_memory_search_zero_hits_total",
    "agentsfleet_process_resident_memory_bytes",
    "agentsfleet_sensitive_request_erased_bytes_total",
    "agentsfleet_sensitive_response_erased_bytes_total",
    "agentsfleet_sensitive_response_write_failures_total",
    "agentsfleet_runner_failures_total",
    "agentsfleet_runner_failures_overflow_total",
    "agentsfleet_runner_executions_total",
    "agentsfleet_runner_last_seen_seconds",
    "agentsfleet_runner_active_leases",
};

fn registryIndexOf(name: []const u8) ?usize {
    for (0..families.METRIC_ID_COUNT) |i| {
        const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
        if (std.mem.eql(u8, meta.name, name)) return i;
    }
    return null;
}

fn registryDeclares(name: []const u8) bool {
    return registryIndexOf(name) != null;
}

fn pinContains(name: []const u8) bool {
    for (CENSUS_PIN) |pinned| {
        if (std.mem.eql(u8, pinned, name)) return true;
    }
    return false;
}

/// Read a repository file relative to the build root; null when the tree is
/// not available from the test's working directory (skip, don't fail).
fn readRepoFile(alloc: std.mem.Allocator, path: []const u8) !?[]u8 {
    return std.Io.Dir.cwd().readFileAlloc(common.globalIo(), path, alloc, .limited(MAX_ASSET_BYTES)) catch |err| switch (err) {
        error.FileNotFound => null,
        else => err,
    };
}

fn isFamilyTokenByte(c: u8) bool {
    return (c >= 'a' and c <= 'z') or c == '_';
}

/// Assert every `agentsfleet_[a-z_]+` token in `text` names a declared family.
fn expectTokensDeclared(text: []const u8, source_name: []const u8) !void {
    var found_any = false;
    var cursor: usize = 0;
    while (std.mem.indexOfPos(u8, text, cursor, FAMILY_TOKEN_PREFIX)) |start| {
        var end = start + FAMILY_TOKEN_PREFIX.len;
        while (end < text.len and isFamilyTokenByte(text[end])) end += 1;
        cursor = end;
        const token = text[start..end];
        found_any = true;
        if (!registryDeclares(token)) {
            std.debug.print("FAIL: {s} queries `{s}`, which no registry family declares\n", .{ source_name, token });
            return error.AssetFamilyUndeclared;
        }
    }
    // An asset with no family token would make this guard vacuous.
    try std.testing.expect(found_any);
}

// ── Dimension 3.1 — every asset-queried family is declared ──────────────────

test "test_every_asset_family_is_declared" {
    const alloc = std.testing.allocator;
    const alerts = (try readRepoFile(alloc, ALERTS_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(alerts);
    const dashboard = (try readRepoFile(alloc, DASHBOARD_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(dashboard);

    try expectTokensDeclared(alerts, ALERTS_PATH);
    try expectTokensDeclared(dashboard, DASHBOARD_PATH);
}

// ── Dimension 6.3 — the documented census and the registry agree ────────────

test "test_census_matches_exported_families" {
    const alloc = std.testing.allocator;

    // The pin table and the registry agree exactly, in both directions.
    try std.testing.expectEqual(families.METRIC_ID_COUNT, CENSUS_PIN.len);
    for (CENSUS_PIN) |name| {
        if (!registryDeclares(name)) {
            std.debug.print("FAIL: census pin `{s}` is not a declared family\n", .{name});
            return error.CensusPinUndeclared;
        }
    }
    for (0..families.METRIC_ID_COUNT) |i| {
        const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
        if (!pinContains(meta.name)) {
            std.debug.print("FAIL: declared family `{s}` is missing from the census pin\n", .{meta.name});
            return error.DeclaredFamilyUnpinned;
        }
    }

    // The architecture census section lists every family and no extras.
    const doc = (try readRepoFile(alloc, ARCHITECTURE_DOC_PATH)) orelse return error.SkipZigTest;
    defer alloc.free(doc);
    const heading = std.mem.indexOf(u8, doc, CENSUS_HEADING) orelse return error.CensusSectionMissing;
    const body_start = heading + CENSUS_HEADING.len;
    const section_end = std.mem.indexOfPos(u8, doc, body_start, SECTION_BREAK) orelse doc.len;
    const section = doc[body_start..section_end];

    // "Appears exactly once" is the census promise: a duplicate row and a
    // missing row must both fail, so track per-family, not by count.
    var documented = [_]bool{false} ** families.METRIC_ID_COUNT;
    var lines = std.mem.splitScalar(u8, section, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, "| `")) continue;
        const name_start = "| `".len;
        const name_end = std.mem.indexOfScalarPos(u8, line, name_start, '`') orelse continue;
        const name = line[name_start..name_end];
        const idx = registryIndexOf(name) orelse {
            std.debug.print("FAIL: census documents `{s}`, which no registry family declares\n", .{name});
            return error.CensusDocumentsUndeclaredFamily;
        };
        if (documented[idx]) {
            std.debug.print("FAIL: census documents `{s}` more than once\n", .{name});
            return error.CensusDuplicateFamily;
        }
        documented[idx] = true;
    }
    for (documented, 0..) |seen, i| {
        if (seen) continue;
        const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
        std.debug.print("FAIL: declared family `{s}` is missing from the documented census\n", .{meta.name});
        return error.CensusIncomplete;
    }
}

// ── Dimension 5.2 — the rebuilt suite covers every family ───────────────────

test "test_rebuilt_suite_covers_every_previously_rendered_family" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var covered = [_]bool{false} ** families.METRIC_ID_COUNT;

    var src_dir = try std.Io.Dir.cwd().openDir(io, "src/agentsfleetd", .{ .iterate = true });
    defer src_dir.close(io);
    var walker = try src_dir.walk(alloc);
    defer walker.deinit();
    while (try walker.next(io)) |entry| {
        if (entry.kind != .file) continue;
        if (!std.mem.endsWith(u8, entry.basename, "_test.zig")) continue;
        const content = try entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(MAX_ASSET_BYTES));
        defer alloc.free(content);
        for (0..families.METRIC_ID_COUNT) |i| {
            if (covered[i]) continue;
            const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
            if (std.mem.indexOf(u8, content, meta.name) != null) covered[i] = true;
        }
    }

    for (covered, 0..) |seen, i| {
        if (seen) continue;
        const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
        std.debug.print("FAIL: no *_test.zig mentions family `{s}`\n", .{meta.name});
        return error.FamilyWithoutTestCoverage;
    }
}

// And the live half of Dimension 5.2: one flush window carries every
// fixed-label family this process can produce — a name mentioned in a test is
// also a series that actually reaches the wire. Evented families need a
// recorded sample and streamed runner families a live slot; both are driven
// and asserted by their own suites (otel_metrics_test / metrics_runner_test).
test "the full fixed-label census is live in one flush window" {
    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    const pool_registered = mrp.snapshot() != null;
    const rss_reported = common.rss.currentBytes() != null;
    for (0..families.METRIC_ID_COUNT) |i| {
        const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
        if (meta.evented or meta.streamed) continue;
        if (!pool_registered and std.mem.startsWith(u8, meta.name, "agentsfleet_redis_pool_")) continue;
        if (!rss_reported and std.mem.eql(u8, meta.name, "agentsfleet_process_resident_memory_bytes")) continue;
        window.expectFamilySample(body, meta.name) catch |err| {
            std.debug.print("FAIL: fixed-label family `{s}` absent from the flush window\n", .{meta.name});
            return err;
        };
    }
}
