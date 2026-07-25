const std = @import("std");
const mc = @import("metrics_counters.zig");
comptime {
    _ = @import("metrics_fleet.zig");
}

// ── SSE hub counters (dropped frames + reconnects) ──────────────────────

// The hub/subscription behaviour tests assert their local mirrors (dropCount,
// delivery recovery); these pin the operator-facing counters themselves so the
// inc call sites, the snapshot fields, and the render lines cannot silently
// go dead while the behaviour suite stays green.

test "incSseDroppedFrames increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incSseDroppedFrames();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.sse_dropped_frames_total + 1, after.sse_dropped_frames_total);
}

test "incSseHubReconnects increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incSseHubReconnects();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.sse_hub_reconnects_total + 1, after.sse_hub_reconnects_total);
}

test "renderPrometheus carries the SSE hub counter lines with snapshot values" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.incSseDroppedFrames();
    mc.incSseHubReconnects();
    const snap = mc.snapshot();
    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);
    var dropped_buf: [128]u8 = undefined;
    const dropped = try std.fmt.bufPrint(&dropped_buf, "agentsfleet_sse_dropped_frames_total {d}", .{snap.sse_dropped_frames_total});
    try std.testing.expect(std.mem.indexOf(u8, output, dropped) != null);
    var reconnects_buf: [128]u8 = undefined;
    const reconnects = try std.fmt.bufPrint(&reconnects_buf, "agentsfleet_sse_hub_reconnects_total {d}", .{snap.sse_hub_reconnects_total});
    try std.testing.expect(std.mem.indexOf(u8, output, reconnects) != null);
}

// ── Backpressure counters + gauges ──────────────────────────────────────

test "incApiBackpressureRejections increments its dedicated counter by 1" {
    const before = mc.snapshot();
    mc.incApiBackpressureRejections();
    const after = mc.snapshot();
    try std.testing.expectEqual(before.api_backpressure_rejections_total + 1, after.api_backpressure_rejections_total);
}

test "in-flight gauges reflect the last stored value" {
    mc.setApiInFlightRequests(7);
    mc.setSseInFlightStreams(4);
    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 7), snap.api_in_flight_requests);
    try std.testing.expectEqual(@as(u64, 4), snap.sse_in_flight_streams);
}

// ── Signup funnel families ──────────────────────────────────────────────

// The signup families had no render coverage, so the namespace normalization
// could have mangled their names with nothing to catch it. The labelled
// `failed` family matters most: its six reasons come from separate snapshot
// fields, and a mis-paired reason would misattribute why signups are failing.
test "renderPrometheus carries every signup funnel family under one namespace" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    const output = try render.renderPrometheus(alloc, true);
    defer alloc.free(output);

    for ([_][]const u8{
        "# TYPE agentsfleet_signup_bootstrapped_total counter\n",
        "# TYPE agentsfleet_signup_replayed_total counter\n",
        "# TYPE agentsfleet_signup_failed_total counter\n",
    }) |type_line| {
        try std.testing.expect(std.mem.indexOf(u8, output, type_line) != null);
    }

    // Each rejection reason renders its own series off its own counter field.
    for ([_][]const u8{
        "bad_sig",          "stale_ts",
        "missing_email",    "db_error",
        "pool_unavailable", "metadata_writeback",
    }) |reason| {
        var buf: [128]u8 = undefined;
        const series = try std.fmt.bufPrint(&buf, "agentsfleet_signup_failed_total{{reason=\"{s}\"}} ", .{reason});
        try std.testing.expect(std.mem.indexOf(u8, output, series) != null);
    }
}

// ── Lease-poll cost + readiness index ───────────────────────────────────
//
// These families exist because the fan-out defect they now measure was
// invisible: nothing on /metrics distinguished an idle poll that cost one Redis
// read from one that walked every fleet on the platform. So the tests pin the
// three properties that make them trustworthy — the render path emits them with
// no datastore reachable, they carry no per-fleet identity, and the depth gauge
// is a sample rather than a running delta.

/// Label keys that would create a series per entity. Rendering any of these on a
/// lease-poll or readiness family is a cardinality leak that outlives the
/// process, so the assertion is against the whole family block, not one line.
const FORBIDDEN_LABEL_KEYS = [_][]const u8{ "fleet", "fleet_id", "workspace", "workspace_id", "tenant", "tenant_id", "runner", "runner_id", "event", "event_id", "lease", "lease_id" };

/// The slice of rendered output belonging to one metric family: from its `# HELP`
/// line to the start of the next family. Asserting against this rather than the
/// whole scrape keeps a neighbouring family's legitimate `runner_id` label from
/// masking a leak here.
fn familyBlock(output: []const u8, name: []const u8) ![]const u8 {
    var head_buf: [160]u8 = undefined;
    const head = try std.fmt.bufPrint(&head_buf, "# HELP {s} ", .{name});
    const start = std.mem.indexOf(u8, output, head) orelse return error.FamilyNotRendered;
    const rest = output[start + head.len ..];
    const end = std.mem.indexOf(u8, rest, "# HELP ") orelse rest.len;
    return rest[0..end];
}

test "the lease-poll cost families render with their snapshot values" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.resetLeasePollMetricsForTest();

    // Two polls of different widths, so the totals and the high-water mark are
    // distinguishable from each other and from the poll count.
    mc.observeLeasePoll(0, 0); // the idle shape: examined nothing, touched no database
    mc.observeLeasePoll(7, 3);

    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 2), snap.lease_polls_total);
    try std.testing.expectEqual(@as(u64, 7), snap.lease_poll_candidates_scanned_total);
    try std.testing.expectEqual(@as(u64, 3), snap.lease_poll_db_roundtrips_total);

    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);
    for ([_][]const u8{
        mc.LEASE_POLLS_NAME,
        mc.CANDIDATES_SCANNED_NAME,
        mc.DB_ROUNDTRIPS_NAME,
        mc.READY_DEPTH_NAME,
        mc.READY_WRITE_FAILURES_NAME,
    }) |name| {
        _ = familyBlock(output, name) catch |err| {
            std.debug.print("family not rendered: {s}\n", .{name});
            return err;
        };
    }
    var line_buf: [160]u8 = undefined;
    const polls_line = try std.fmt.bufPrint(&line_buf, "{s} 2", .{mc.LEASE_POLLS_NAME});
    try std.testing.expect(std.mem.indexOf(u8, output, polls_line) != null);
}

test "an idle poll still contributes a sample" {
    // The idle case is the entire point of the family. If `observeLeasePoll` were
    // skipped when a poll found nothing, the poll count would track only busy
    // polls and the derived mean fan-out would read as if idle polls never
    // happened — hiding exactly the cost this workstream removed.
    mc.resetLeasePollMetricsForTest();
    mc.observeLeasePoll(0, 0);
    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 1), snap.lease_polls_total);
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_candidates_scanned_total);
    try std.testing.expectEqual(@as(u64, 0), snap.lease_poll_db_roundtrips_total);
}

test "no lease-poll or readiness family carries a per-entity label" {
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.resetLeasePollMetricsForTest();
    mc.observeLeasePoll(4, 2);
    mc.setReadyIndexDepth(9);
    mc.incReadyWriteFailure();

    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);

    for ([_][]const u8{
        mc.LEASE_POLLS_NAME,
        mc.CANDIDATES_SCANNED_NAME,
        mc.DB_ROUNDTRIPS_NAME,
        mc.READY_DEPTH_NAME,
        mc.READY_WRITE_FAILURES_NAME,
    }) |name| {
        const block = try familyBlock(output, name);
        // Every one of these is wholly unlabelled, so a brace anywhere in its
        // block is a cardinality leak — no exceptions to check around.
        try std.testing.expect(std.mem.indexOfScalar(u8, block, '{') == null);
        for (FORBIDDEN_LABEL_KEYS) |key| {
            var probe_buf: [64]u8 = undefined;
            const probe = try std.fmt.bufPrint(&probe_buf, "{s}=\"", .{key});
            try std.testing.expect(std.mem.indexOf(u8, block, probe) == null);
        }
    }
}

test "readiness write failures render and move with observed state" {
    mc.resetLeasePollMetricsForTest();
    mc.incReadyWriteFailure();
    mc.incReadyWriteFailure();
    mc.incReadyWriteFailure();
    try std.testing.expectEqual(@as(u64, 3), mc.snapshot().fleet_ready_write_failures_total);

    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);
    var line_buf: [160]u8 = undefined;
    const line = try std.fmt.bufPrint(&line_buf, "{s} 3", .{mc.READY_WRITE_FAILURES_NAME});
    try std.testing.expect(std.mem.indexOf(u8, output, line) != null);
}

test "readiness depth is a sample the caller overwrites, never an accumulator" {
    // The index is one hash shared by every replica, so depth cannot be tracked
    // by incrementing on mark and decrementing on clear — cross-replica writes
    // and restarts would drift it permanently from what it claims to measure.
    // The only writer is this setter, and setting it twice must not sum.
    mc.resetLeasePollMetricsForTest();
    mc.setReadyIndexDepth(3);
    try std.testing.expectEqual(@as(u64, 3), mc.snapshot().fleet_ready_depth);
    mc.setReadyIndexDepth(3);
    try std.testing.expectEqual(@as(u64, 3), mc.snapshot().fleet_ready_depth);
    // And it tracks downward, which an accumulator could not do.
    mc.setReadyIndexDepth(1);
    try std.testing.expectEqual(@as(u64, 1), mc.snapshot().fleet_ready_depth);
    mc.setReadyIndexDepth(0);
    try std.testing.expectEqual(@as(u64, 0), mc.snapshot().fleet_ready_depth);
}

test "the render path needs no datastore" {
    // /metrics must stay healthy exactly when Postgres or Redis is not, so the
    // scrape reads the in-memory snapshot only. This test runs with no pool and
    // no Redis client in scope at all — it passing is the proof.
    const alloc = std.testing.allocator;
    const render = @import("metrics_render.zig");
    mc.resetLeasePollMetricsForTest();
    mc.setReadyIndexDepth(5);
    const output = try render.renderPrometheus(alloc, false);
    defer alloc.free(output);
    var depth_buf: [160]u8 = undefined;
    const depth_line = try std.fmt.bufPrint(&depth_buf, "{s} 5", .{mc.READY_DEPTH_NAME});
    try std.testing.expect(std.mem.indexOf(u8, output, depth_line) != null);
}
