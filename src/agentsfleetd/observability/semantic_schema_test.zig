//! Drift test binding what this process emits to one telemetry schema.
//!
//! Two things can silently disagree: the semantic registry in `semconv.zig`
//! (what the OpenTelemetry Protocol payload declares) and the Prometheus
//! families `metrics_render.zig` writes to `/metrics`. Nothing at build time
//! couples them, so a rename on one side leaves the other spelling a name the
//! schema retired.
//!
//! This replaces the retired `audits/signal-routing.sh`, which pinned the same
//! intent by grepping prose and source literals for exact strings. That shape
//! passed whenever someone edited both the source and the grep pattern. Here the
//! exposition body is rendered and every family in it is checked, so a new
//! family is covered the moment it is written rather than when someone
//! remembers to extend a pattern list.
//!
//! Grafana is deliberately absent. The dashboards were deleted with this
//! cutover and are owned by a separate workstream; a dashboard-query drift test
//! belongs there, against files that exist.

const std = @import("std");
const semconv = @import("semconv.zig");
const metrics_render = @import("metrics_render.zig");
const metrics_runner = @import("metrics_runner.zig");
const metrics_memory = @import("metrics_memory.zig");
const metrics_otel = @import("metrics_otel.zig");

/// The one namespace every Prometheus family this process renders must carry.
const NAMESPACE_PREFIX = "agentsfleet_";
/// The superseded namespace. `fleet_id`, log event names, `EventKind` tags, and
/// the Redis consumer group share this spelling and are NOT metric families —
/// this constant is only ever matched against a family name or a quoted literal
/// in a renderer, never against free text.
const SUPERSEDED_PREFIX = "fleet_";
const TYPE_LINE_PREFIX = "# TYPE ";

/// Collect every family name from the `# TYPE <name> <kind>` lines of a rendered
/// exposition body. Caller frees the list; items borrow from `body`.
fn renderedFamilies(alloc: std.mem.Allocator, body: []const u8) !std.ArrayList([]const u8) {
    var names: std.ArrayList([]const u8) = .empty;
    errdefer names.deinit(alloc);

    var lines = std.mem.splitScalar(u8, body, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, TYPE_LINE_PREFIX)) continue;
        const rest = line[TYPE_LINE_PREFIX.len..];
        const end = std.mem.indexOfScalar(u8, rest, ' ') orelse rest.len;
        try names.append(alloc, rest[0..end]);
    }
    return names;
}

/// Render `/metrics` with the lazily-gated subsystems switched on, so the body
/// carries the runner and durable-memory families too. Redis-pool families need
/// a live Pool and are asserted by the pool integration test instead; the
/// renderer-source test below covers them without standing one up.
fn renderFullBody(alloc: std.mem.Allocator) ![]u8 {
    metrics_runner.resetForTest();
    metrics_runner.incRunnerFailure("semantic-schema-probe", null);
    metrics_memory.incMemoryCaptured(1);
    return metrics_render.renderPrometheus(alloc, true);
}

// ── One namespace across the whole exposition ───────────────────────────────

test "test_prometheus_families_share_one_namespace" {
    const alloc = std.testing.allocator;

    const body = try renderFullBody(alloc);
    defer alloc.free(body);
    defer metrics_runner.resetForTest();

    var families = try renderedFamilies(alloc, body);
    defer families.deinit(alloc);

    // Guard the guard: an empty body would satisfy the prefix loop vacuously.
    try std.testing.expect(families.items.len > 0);
    for (families.items) |name| {
        if (!std.mem.startsWith(u8, name, NAMESPACE_PREFIX)) {
            std.debug.print("FAIL: family `{s}` is outside the `{s}` namespace\n", .{ name, NAMESPACE_PREFIX });
            return error.MetricFamilyOutsideNamespace;
        }
    }
}

// The rendered body cannot see families whose block is gated behind a live
// dependency (the Redis pool needs a real connection). Reading the renderer
// sources directly covers those without standing up the dependency.
test "test_no_renderer_source_declares_a_superseded_family_name" {
    const RENDERER_SOURCES = [_][]const u8{
        @embedFile("metrics_render.zig"),
        @embedFile("metrics_memory.zig"),
        @embedFile("metrics_sensitive_memory.zig"),
        @embedFile("metrics_runner.zig"),
        @embedFile("metrics_otel.zig"),
        @embedFile("metrics_trace.zig"),
    };
    // A family name always reaches the writer as a quoted literal, so the quote
    // anchor keeps `fleet_id` fields and log event names out of the match.
    const QUOTED_SUPERSEDED = "\"" ++ SUPERSEDED_PREFIX;

    for (RENDERER_SOURCES) |source| {
        if (std.mem.indexOf(u8, source, QUOTED_SUPERSEDED)) |hit| {
            const line_end = std.mem.indexOfScalarPos(u8, source, hit, '\n') orelse source.len;
            std.debug.print("FAIL: renderer declares `{s}`\n", .{source[hit..line_end]});
            return error.RendererDeclaresSupersededFamily;
        }
    }
}

// ── No superseded OTLP name survives anywhere live ──────────────────────────

test "test_semantic_schema_has_no_live_legacy_aliases" {
    const alloc = std.testing.allocator;

    const body = try renderFullBody(alloc);
    defer alloc.free(body);
    defer metrics_runner.resetForTest();

    // The rejected list holds both the superseded product spellings and the
    // GenAI client-call names whose measured boundary this process cannot
    // observe. Neither may appear on the wire under either transport.
    const payload = @embedFile("otlp_metrics.json");
    for (semconv.REJECTED_METRIC_NAMES) |rejected| {
        if (std.mem.indexOf(u8, payload, rejected) != null) {
            std.debug.print("FAIL: pinned OTLP fixture still carries `{s}`\n", .{rejected});
            return error.FixtureCarriesSupersededMetric;
        }

        // Prometheus translates dots to underscores, so a superseded OTLP name
        // could resurface in the scrape body under its translated spelling.
        const translated = try alloc.dupe(u8, rejected);
        defer alloc.free(translated);
        std.mem.replaceScalar(u8, translated, '.', '_');
        if (std.mem.indexOf(u8, body, translated) != null) {
            std.debug.print("FAIL: /metrics still renders `{s}`\n", .{translated});
            return error.ExpositionRendersSupersededMetric;
        }
    }
}

test "test_attribution_omissions_are_visible_in_the_scrape_body" {
    const alloc = std.testing.allocator;
    const body = try renderFullBody(alloc);
    defer alloc.free(body);
    defer metrics_runner.resetForTest();

    // Bounded model attribution is only honest if the omissions it takes are
    // observable; the counter must render even while every value is zero, or an
    // operator reads a gap in model coverage as an idle model.
    try std.testing.expect(std.mem.indexOf(u8, body, metrics_otel.ATTRIBUTE_OMITTED_NAME) != null);
    for (metrics_otel.OMITTED_ATTRIBUTES) |attribute| {
        try std.testing.expect(std.mem.indexOf(u8, body, attribute.label()) != null);
    }
    for (metrics_otel.OMISSION_REASONS) |reason| {
        try std.testing.expect(std.mem.indexOf(u8, body, reason.label()) != null);
    }
}
