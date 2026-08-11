//! Drift test binding what this process emits to one telemetry schema.
//!
//! Two things can silently disagree: the semantic registry in `semconv.zig`
//! (names and rejections) and the closed family registry in
//! `otel_metrics_families.zig` (what the OTLP payload actually declares).
//! The guards below walk the DECLARED registry — the single source the wire
//! serializer reads — so a family cannot exist on the wire outside the
//! namespace or under a superseded spelling.
//!
//! Grafana asset drift (dashboards/alerts naming undeclared families) is
//! guarded separately by otel_metrics_census_test.zig.

const std = @import("std");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const metrics_otel = @import("metrics_otel.zig");
const window = @import("otel_metrics_window_test.zig");

/// The one namespace every underscore-spelled runtime family must carry.
const NAMESPACE_PREFIX = "agentsfleet_";
/// The superseded namespace. `fleet_id`, log event names, `EventKind` tags, and
/// the Redis consumer group share this spelling and are NOT metric families —
/// this constant is only ever matched against a family name or a quoted literal
/// in a name-declaring module, never against free text.
const SUPERSEDED_PREFIX = "fleet_";

/// The namespace rule, factored so the negative cases below can prove the
/// guard rejects rather than merely that today's registry passes. Cost
/// families use their dotted semantic-convention names; every other family
/// carries the project's underscore namespace.
fn familyNameOk(cost: bool, name: []const u8) bool {
    if (cost) {
        return std.mem.indexOfScalar(u8, name, '.') != null and
            !std.mem.startsWith(u8, name, NAMESPACE_PREFIX);
    }
    return std.mem.startsWith(u8, name, NAMESPACE_PREFIX);
}

// ── Dimension 5.3 — one namespace across the declared registry ──────────────

test "test_namespace_guard_runs_against_the_payload_source" {
    for (0..families.METRIC_ID_COUNT) |i| {
        const id: families.MetricId = @enumFromInt(i);
        const meta = families.metaFor(id);
        if (!familyNameOk(meta.cost, meta.name)) {
            std.debug.print("FAIL: family `{s}` is outside the declared namespace rule\n", .{meta.name});
            return error.MetricFamilyOutsideNamespace;
        }
        // The superseded prefix may never open a family name in either shape.
        try std.testing.expect(!std.mem.startsWith(u8, meta.name, SUPERSEDED_PREFIX));
    }
    // The guard rejects, not merely tolerates: a family outside the project
    // namespace fails it, in both the runtime and the cost shape.
    try std.testing.expect(!familyNameOk(false, "wild_metric_total"));
    try std.testing.expect(!familyNameOk(false, "fleet_reconcile_running"));
    try std.testing.expect(!familyNameOk(true, "agentsfleet_snake_spelling_total"));
}

// ── Dimension 5.4 — no superseded name survives in the exporter ─────────────

test "test_superseded_name_guard_scans_the_exporter" {
    const alloc = std.testing.allocator;

    // The registry source is the exporter's single name authority; a rejected
    // name reappearing there as a literal is the regression this guard exists
    // to catch since the renderer sources it used to scan are deleted.
    const registry_source = @embedFile("otel_metrics_families.zig");
    // The pinned OTLP fixture is the serialized wire shape; rejected names may
    // not ride it either.
    const fixture = @embedFile("otlp_metrics.json");

    for (semconv.REJECTED_METRIC_NAMES) |rejected| {
        if (std.mem.indexOf(u8, registry_source, rejected) != null) {
            std.debug.print("FAIL: exporter registry carries `{s}`\n", .{rejected});
            return error.ExporterCarriesSupersededMetric;
        }
        if (std.mem.indexOf(u8, fixture, rejected) != null) {
            std.debug.print("FAIL: pinned OTLP fixture still carries `{s}`\n", .{rejected});
            return error.FixtureCarriesSupersededMetric;
        }

        // And no DECLARED name is a rejected name — introducing one into the
        // registry fails here even when it arrives through a semconv constant
        // rather than a literal. The underscore translation is checked too, so
        // a superseded dotted name cannot resurface in the runtime spelling.
        const translated = try alloc.dupe(u8, rejected);
        defer alloc.free(translated);
        std.mem.replaceScalar(u8, translated, '.', '_');
        for (0..families.METRIC_ID_COUNT) |i| {
            const meta = families.metaFor(@as(families.MetricId, @enumFromInt(i)));
            try std.testing.expect(!std.mem.eql(u8, meta.name, rejected));
            try std.testing.expect(!std.mem.eql(u8, meta.name, translated));
        }
    }
}

// The name-declaring modules cannot spell a family under the superseded
// namespace even where the declaration is gated behind a live dependency
// (the Redis pool families need a real connection to carry values).
test "test_no_name_declaring_source_carries_a_superseded_family_prefix" {
    const NAME_SOURCES = [_][]const u8{
        @embedFile("otel_metrics_families.zig"),
        @embedFile("metrics_counters.zig"),
        @embedFile("metrics_memory.zig"),
        @embedFile("metrics_sensitive_memory.zig"),
        @embedFile("metrics_runner.zig"),
        @embedFile("metrics_otel.zig"),
        @embedFile("metrics_trace.zig"),
        @embedFile("library_stages.zig"),
    };
    // A family name always reaches a declaration as a quoted literal, so the
    // quote anchor keeps `fleet_id` fields and log event names out of the match.
    const QUOTED_SUPERSEDED = "\"" ++ SUPERSEDED_PREFIX;

    for (NAME_SOURCES) |source| {
        if (std.mem.indexOf(u8, source, QUOTED_SUPERSEDED)) |hit| {
            const line_end = std.mem.indexOfScalarPos(u8, source, hit, '\n') orelse source.len;
            std.debug.print("FAIL: name-declaring module carries `{s}`\n", .{source[hit..line_end]});
            return error.SourceDeclaresSupersededFamily;
        }
    }
}

test "test_attribution_omissions_are_visible_in_the_exported_window" {
    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // Bounded model attribution is only honest if the omissions it takes are
    // observable; the counter must export even while every value is zero, or an
    // operator reads a gap in model coverage as an idle model.
    try window.expectFamilySample(body, metrics_otel.ATTRIBUTE_OMITTED_NAME);
    for (metrics_otel.OMITTED_ATTRIBUTES) |attribute| {
        var attr_buf: [96]u8 = undefined;
        try window.expectFamilyWith(body, metrics_otel.ATTRIBUTE_OMITTED_NAME, &.{
            try window.attrFragment(&attr_buf, "attribute", attribute.label()),
        });
    }
    for (metrics_otel.OMISSION_REASONS) |reason| {
        var reason_buf: [96]u8 = undefined;
        try window.expectFamilyWith(body, metrics_otel.ATTRIBUTE_OMITTED_NAME, &.{
            try window.attrFragment(&reason_buf, "reason", reason.label()),
        });
    }
}
