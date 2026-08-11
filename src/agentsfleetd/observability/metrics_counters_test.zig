const std = @import("std");
const mc = @import("metrics_counters.zig");
const window = @import("otel_metrics_window_test.zig");

// ── Fleet trigger counter ───────────────────────────────────────────────

test "single fleet trigger increments the counter by exactly one" {
    const before = mc.snapshot().fleet_triggered_total;
    mc.incFleetsTriggered();
    const after = mc.snapshot().fleet_triggered_total;
    try std.testing.expectEqual(@as(u64, 1), after - before);
}

// ── SSE hub counters (dropped frames + reconnects) ──────────────────────

// The hub/subscription behaviour tests assert their local mirrors (dropCount,
// delivery recovery); these pin the operator-facing counters themselves so the
// inc call sites, the snapshot fields, and the exported series cannot silently
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

test "the exported window carries the SSE hub counters with snapshot values" {
    const alloc = std.testing.allocator;
    mc.incSseDroppedFrames();
    mc.incSseHubReconnects();
    const snap = mc.snapshot();
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(
        @as(i64, @intCast(snap.sse_dropped_frames_total)),
        try window.familyValueWith(body, "agentsfleet_sse_dropped_frames_total", &.{}), // pin test: literal is the contract
    );
    try std.testing.expectEqual(
        @as(i64, @intCast(snap.sse_hub_reconnects_total)),
        try window.familyValueWith(body, "agentsfleet_sse_hub_reconnects_total", &.{}), // pin test: literal is the contract
    );
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
    mc.setApiInFlightRequests(0);
    mc.setSseInFlightStreams(0);
}

// Dimension 3.2 — saturation levels export as gauges carrying the live value,
// and a second set inside the SAME flush window folds to the newest value
// (last-value-wins end to end: setter → snapshot → collector → gauge point).
test "test_saturation_families_export_current_level" {
    const alloc = std.testing.allocator;
    const IN_FLIGHT_FAMILY = "agentsfleet_api_in_flight_requests"; // pin test: literal is the contract

    mc.setApiInFlightRequests(3);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    // A level serializes in the gauge shape, valued exactly what was set.
    try window.expectFamilyWith(body, IN_FLIGHT_FAMILY, &.{"\"gauge\":{\"dataPoints\""});
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, IN_FLIGHT_FAMILY, &.{}));

    // Two sets in one window: the newest level wins, never their sum.
    mc.setApiInFlightRequests(3);
    mc.setApiInFlightRequests(1);
    const body2 = try window.flushWindowJson(alloc);
    defer alloc.free(body2);
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body2, IN_FLIGHT_FAMILY, &.{}));

    mc.setApiInFlightRequests(0);
}

// Dimension 3.3 — a counter family exports as a monotonic CUMULATIVE sum
// (stamped from process start, needing no per-flush delta memo): two
// increments arrive on the wire as a sum valued exactly two.
test "test_cumulative_families_export_as_sums" {
    const alloc = std.testing.allocator;
    mc.resetLeasePollMetricsForTest();
    defer mc.resetLeasePollMetricsForTest();
    mc.incReadyWriteFailure();
    mc.incReadyWriteFailure();

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    const SUM_CUMULATIVE_MONOTONIC = "\"sum\":{\"aggregationTemporality\":2,\"isMonotonic\":true"; // pin test: literal is the contract
    try window.expectFamilyWith(body, mc.READY_WRITE_FAILURES_NAME, &.{SUM_CUMULATIVE_MONOTONIC});
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, mc.READY_WRITE_FAILURES_NAME, &.{}));
}

// ── Signup funnel families ──────────────────────────────────────────────

// The signup families had no exported coverage, so a name drift could ship
// with nothing to catch it. The labelled `failed` family matters most: its six
// reasons come from separate snapshot fields, and a mis-paired reason would
// misattribute why signups are failing.
test "the window carries every signup funnel family with its reason labels" {
    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    try window.expectFamilySample(body, "agentsfleet_signup_bootstrapped_total"); // pin test: literal is the contract
    try window.expectFamilySample(body, "agentsfleet_signup_replayed_total"); // pin test: literal is the contract

    // Each rejection reason exports its own series, keyed by the enum the
    // registry declares the dimension off.
    inline for (@typeInfo(mc.SignupFailReason).@"enum".fields) |reason| {
        var frag_buf: [96]u8 = undefined;
        const reason_attr = try window.attrFragment(&frag_buf, "reason", reason.name);
        try window.expectFamilyWith(body, "agentsfleet_signup_failed_total", &.{reason_attr}); // pin test: literal is the contract
    }
}

// Dimension: a reason value physically binds to its declared label — writing
// one enum member moves exactly that member's cell and no sibling's.
test "test_reason_labels_bind_by_enum_not_order" {
    const before = mc.snapshot();
    mc.incSignupFailed(.stale_ts);
    const after = mc.snapshot();
    try std.testing.expectEqual(before.signup_failed_stale_ts_total + 1, after.signup_failed_stale_ts_total);
    try std.testing.expectEqual(before.signup_failed_bad_sig_total, after.signup_failed_bad_sig_total);
    try std.testing.expectEqual(before.signup_failed_missing_email_total, after.signup_failed_missing_email_total);
    try std.testing.expectEqual(before.signup_failed_db_error_total, after.signup_failed_db_error_total);
    try std.testing.expectEqual(before.signup_failed_pool_unavailable_total, after.signup_failed_pool_unavailable_total);
    try std.testing.expectEqual(before.signup_failed_metadata_writeback_total, after.signup_failed_metadata_writeback_total);
}

// ── Lease-poll cost + readiness index ───────────────────────────────────
//
// These families exist because the fan-out defect they now measure was
// invisible: nothing exported distinguished an idle poll that cost one Redis
// read from one that walked every fleet on the platform. So the tests pin the
// three properties that make them trustworthy — the export path emits them with
// no datastore reachable, they carry no per-entity label, and the depth gauge
// is a sample rather than a running delta.

/// Label keys that would create a series per entity. Exporting any of these on
/// a lease-poll or readiness family is a cardinality leak that outlives the
/// process, so the assertion is against every dataPoint of the family.
const FORBIDDEN_LABEL_KEYS = [_][]const u8{ "fleet", "fleet_id", "workspace", "workspace_id", "tenant", "tenant_id", "runner", "runner_id", "event", "event_id", "lease", "lease_id" };

const LEASE_READY_FAMILIES = [_][]const u8{
    mc.LEASE_POLLS_NAME,
    mc.CANDIDATES_SCANNED_NAME,
    mc.DB_ROUNDTRIPS_NAME,
    mc.READY_DEPTH_NAME,
    mc.READY_WRITE_FAILURES_NAME,
};

test "the lease-poll cost families export with their snapshot values" {
    const alloc = std.testing.allocator;
    mc.resetLeasePollMetricsForTest();
    defer mc.resetLeasePollMetricsForTest();

    // Two polls of different widths, so the totals are distinguishable from
    // each other and from the poll count.
    mc.observeLeasePoll(0, 0); // the idle shape: examined nothing, touched no database
    mc.observeLeasePoll(7, 3);

    const snap = mc.snapshot();
    try std.testing.expectEqual(@as(u64, 2), snap.lease_polls_total);
    try std.testing.expectEqual(@as(u64, 7), snap.lease_poll_candidates_scanned_total);
    try std.testing.expectEqual(@as(u64, 3), snap.lease_poll_db_roundtrips_total);

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    for (LEASE_READY_FAMILIES) |name| {
        window.expectFamilySample(body, name) catch |err| {
            std.debug.print("family not exported: {s}\n", .{name});
            return err;
        };
    }
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, mc.LEASE_POLLS_NAME, &.{}));
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
    mc.resetLeasePollMetricsForTest();
}

test "no lease-poll or readiness family carries a per-entity label" {
    const alloc = std.testing.allocator;
    mc.resetLeasePollMetricsForTest();
    defer mc.resetLeasePollMetricsForTest();
    mc.observeLeasePoll(4, 2);
    mc.setReadyIndexDepth(9);
    mc.incReadyWriteFailure();

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    for (LEASE_READY_FAMILIES) |name| {
        // Every one of these is wholly unlabelled: each dataPoint carries an
        // empty attribute list, so a forbidden key cannot ride any of them.
        try window.expectFamilyWith(body, name, &.{window.NO_ATTRIBUTES});
        for (FORBIDDEN_LABEL_KEYS) |key| {
            var probe_buf: [64]u8 = undefined;
            const probe = try std.fmt.bufPrint(&probe_buf, "\"key\":\"{s}\"", .{key});
            try window.expectNoFamilyWith(body, name, &.{probe});
        }
    }
}

test "readiness write failures export and move with observed state" {
    mc.resetLeasePollMetricsForTest();
    defer mc.resetLeasePollMetricsForTest();
    mc.incReadyWriteFailure();
    mc.incReadyWriteFailure();
    mc.incReadyWriteFailure();
    try std.testing.expectEqual(@as(u64, 3), mc.snapshot().fleet_ready_write_failures_total);

    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, mc.READY_WRITE_FAILURES_NAME, &.{}));
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

test "the export path needs no datastore" {
    // The exported window must stay healthy exactly when Postgres or Redis is
    // not, so the flush reads in-memory snapshots only. This test runs with no
    // pool and no Redis client in scope at all — it passing is the proof.
    const alloc = std.testing.allocator;
    mc.resetLeasePollMetricsForTest();
    defer mc.resetLeasePollMetricsForTest();
    mc.setReadyIndexDepth(5);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 5), try window.familyValueWith(body, mc.READY_DEPTH_NAME, &.{}));
}
