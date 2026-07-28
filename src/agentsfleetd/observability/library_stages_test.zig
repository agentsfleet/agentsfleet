//! Unit tier for §1 — `test_library_stage_enum_is_closed`, plus the fan-out
//! rules that keep the series budget fixed.
//!
//! The closedness test is deliberately STRUCTURAL rather than value-driven. A
//! test that records one observation and greps the render for a sentinel proves
//! the one path it drove; §1's claim is about the type — that no caller CAN
//! supply a free-form label — and only reading the type decides that.

const std = @import("std");
const testing = std.testing;

const stages = @import("library_stages.zig");
const render = @import("metrics_render.zig");

// Every dimension of the observation is an enum, so a free-form string cannot
// reach a label. This fails at `@typeInfo` time the moment someone widens a
// field to `[]const u8` for "just this one case", which is exactly how a
// fixed-cardinality schema becomes an unbounded one.
test "test_library_stage_enum_is_closed — every label-bearing field is a closed enum" {
    const fields = @typeInfo(stages.LibraryObservation).@"struct".fields;

    inline for (fields) |f| {
        const T = f.type;
        const info = @typeInfo(T);
        const is_label = comptime std.mem.eql(u8, f.name, "surface") or
            std.mem.eql(u8, f.name, "stage") or
            std.mem.eql(u8, f.name, "outcome") or
            std.mem.eql(u8, f.name, "cache") or
            std.mem.eql(u8, f.name, "pool_result");
        if (!is_label) continue;

        // `pool_result` is optional; unwrap before classifying.
        const Inner = switch (info) {
            .optional => |o| o.child,
            else => T,
        };
        try testing.expect(@typeInfo(Inner) == .@"enum");
    }
}

// The numeric fields carry numbers and nothing else. A `[]const u8` here would
// be a free-form value reaching an artifact by another door.
test "test_library_stage_enum_is_closed — numeric fields are numeric" {
    const fields = @typeInfo(stages.LibraryObservation).@"struct".fields;
    inline for (fields) |f| {
        const is_numeric = comptime std.mem.eql(u8, f.name, "duration_ns") or
            std.mem.eql(u8, f.name, "count") or
            std.mem.eql(u8, f.name, "bytes");
        if (!is_numeric) continue;
        const info = @typeInfo(f.type);
        const Inner = switch (info) {
            .optional => |o| o.child,
            else => f.type,
        };
        try testing.expect(@typeInfo(Inner) == .int);
    }
}

// §1 lists the members by name. Pinning them here means a rename or a deletion
// is a test failure rather than a silently renamed dashboard series.
test "test_library_stage_enum_is_closed — the members are exactly the ones §1 names" {
    try testing.expectEqual(@as(usize, 3), stages.SURFACE_LABELS.len);
    try testing.expectEqual(@as(usize, 10), stages.STAGE_LABELS.len);
    try testing.expectEqual(@as(usize, 9), stages.OUTCOME_LABELS.len);
    try testing.expectEqual(@as(usize, 5), stages.CACHE_LABELS.len);
    try testing.expectEqual(@as(usize, 4), stages.POOL_RESULT_LABELS.len);

    // `error` is a keyword; the member is `@"error"` and its label must still
    // read `error`, because that is the value §1 names and a dashboard queries.
    try testing.expectEqualStrings("error", @tagName(stages.PoolResult.@"error"));
    try testing.expectEqualStrings("secret_project", @tagName(stages.Stage.secret_project));
}

// `fleet_detail` left with the route that was stripped. Asserting its ABSENCE
// keeps it from being restored by reflex: a member no producer can emit is a
// permanently empty series on every dashboard that groups by surface.
test "test_library_stage_enum_is_closed — fleet_detail is not a surface" {
    inline for (stages.SURFACE_LABELS) |label| {
        try testing.expect(!std.mem.eql(u8, label, "fleet_detail"));
    }
}

// The whole cardinality argument is this number. The comptime assertion in the
// module fails the BUILD if it moves; this fails the TEST, which is what shows
// up in a diff review with the enum change that caused it.
test "test_library_stage_enum_is_closed — the series budget is fixed at 102" {
    // pin test: literal is the contract
    try testing.expectEqual(@as(usize, 102), stages.TOTAL_SERIES);
}

test "observeStage feeds the stage family and leaves read outcome untouched" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeStage(.{
        .surface = .tenant_models,
        .stage = .sql,
        .outcome = .ok,
        .duration_ns = 1_500,
    });

    const snap = stages.snapshot();
    const sql_idx = @intFromEnum(stages.Stage.sql);
    const tenant_idx = @intFromEnum(stages.Surface.tenant_models);
    try testing.expectEqual(@as(u64, 1_500), snap.stages[tenant_idx][sql_idx].duration_ns);
    try testing.expectEqual(@as(u64, 1), snap.stages[tenant_idx][sql_idx].count);

    // The per-request family must NOT move: incrementing it per stage is how a
    // ten-stage read reports ten reads.
    for (snap.read_outcomes[tenant_idx]) |v| try testing.expectEqual(@as(u64, 0), v);
}

test "a stage that states no pool or cache dimension records neither" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeStage(.{
        .surface = .global_models,
        .stage = .map,
        .outcome = .ok,
        .duration_ns = 10,
    });

    const snap = stages.snapshot();
    for (snap.pool_results) |v| try testing.expectEqual(@as(u64, 0), v);
    for (snap.cache_outcomes) |v| try testing.expectEqual(@as(u64, 0), v);
}

test "pool and cache dimensions record only when the stage states them" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeStage(.{
        .surface = .fleet_summary,
        .stage = .pool_wait,
        .outcome = .timeout,
        .pool_result = .timeout,
        .duration_ns = 42,
    });
    stages.observeStage(.{
        .surface = .global_models,
        .stage = .cache_lookup,
        .outcome = .ok,
        .cache = .hit,
        .duration_ns = 7,
    });

    const snap = stages.snapshot();
    try testing.expectEqual(@as(u64, 1), snap.pool_results[@intFromEnum(stages.PoolResult.timeout)]);
    try testing.expectEqual(@as(u64, 1), snap.cache_outcomes[@intFromEnum(stages.Cache.hit)]);
    // `not_applicable` is the default and must never become a counted series —
    // it is the absence of a cache decision, not a fifth kind of decision.
    try testing.expectEqual(@as(u64, 0), snap.cache_outcomes[@intFromEnum(stages.Cache.not_applicable)]);
}

test "observeReadOutcome increments exactly one surface/outcome cell" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeReadOutcome(.tenant_models, .forbidden);
    stages.observeReadOutcome(.tenant_models, .forbidden);

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);
    try testing.expectEqual(@as(u64, 2), snap.read_outcomes[s][@intFromEnum(stages.Outcome.forbidden)]);
    try testing.expectEqual(@as(u64, 0), snap.read_outcomes[s][@intFromEnum(stages.Outcome.ok)]);
    // A different surface shares no cell with this one.
    const other = @intFromEnum(stages.Surface.global_models);
    try testing.expectEqual(@as(u64, 0), snap.read_outcomes[other][@intFromEnum(stages.Outcome.forbidden)]);
}

test "bytes and results accumulate per surface" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeStage(.{
        .surface = .tenant_models,
        .stage = .serialize,
        .outcome = .ok,
        .duration_ns = 3,
        .bytes = 4096,
        .count = 100,
    });

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);
    try testing.expectEqual(@as(u64, 4096), snap.payload_bytes[s]);
    try testing.expectEqual(@as(u64, 100), snap.results[s]);
}

test "resetForTest returns every family to zero" {
    stages.resetForTest();

    stages.observeStage(.{
        .surface = .fleet_summary,
        .stage = .authorize,
        .outcome = .ok,
        .cache = .miss,
        .pool_result = .acquired,
        .duration_ns = 99,
        .bytes = 1,
        .count = 1,
    });
    stages.observeReadOutcome(.fleet_summary, .ok);

    stages.resetForTest();
    const snap = stages.snapshot();
    for (snap.stages) |row| for (row) |cell| {
        try testing.expectEqual(@as(u64, 0), cell.duration_ns);
        try testing.expectEqual(@as(u64, 0), cell.count);
    };
    for (snap.read_outcomes) |row| for (row) |v| try testing.expectEqual(@as(u64, 0), v);
    for (snap.pool_results) |v| try testing.expectEqual(@as(u64, 0), v);
    for (snap.cache_outcomes) |v| try testing.expectEqual(@as(u64, 0), v);
    for (snap.payload_bytes) |v| try testing.expectEqual(@as(u64, 0), v);
    for (snap.results) |v| try testing.expectEqual(@as(u64, 0), v);
}

// The exposition carries every family, and — the half that matters for §1
// Dimension 1.2 — carries nothing else. Rendering the WHOLE scrape rather than
// one family is deliberate: a leak reaches an operator through whatever line
// happens to carry it, so the assertion has to read the same bytes the scraper
// does.
test "test_library_evidence_is_secret_and_metadata_free — the scrape emits closed labels only" {
    stages.resetForTest();
    defer stages.resetForTest();

    stages.observeStage(.{
        .surface = .tenant_models,
        .stage = .secret_project,
        .outcome = .ok,
        .duration_ns = std.time.ns_per_s,
        .bytes = 512,
        .count = 7,
    });
    stages.observeReadOutcome(.tenant_models, .ok);

    const alloc = testing.allocator;
    const body = try render.renderPrometheus(alloc, true);
    defer alloc.free(body);

    // Every family is present.
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.STAGE_DURATION_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.STAGE_OBSERVATIONS_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.READ_OUTCOME_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.POOL_RESULT_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.CACHE_OUTCOME_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.PAYLOAD_BYTES_NAME));
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, stages.RESULTS_NAME));

    // One second recorded renders as one second, so the ns->s divide is not
    // silently dropping or scaling the measurement.
    try testing.expect(std.mem.containsAtLeast(u8, body, 1, "stage=\"secret_project\"} 1"));

    // Every label on every library line is checked against an ALLOW list, not a
    // deny list. A deny list of secret-shaped substrings cannot decide this
    // surface: `stage="sql"` and `stage="secret_project"` are legitimate closed
    // values that contain "sql" and "secret", so a substring scan either
    // false-fires on them or is weakened until it catches nothing. Requiring
    // each key to be one of §1's five and each value to be a member of that
    // key's enum is the property §1 actually states, and it rejects a
    // free-form value no deny list would have thought to spell.
    var lines = std.mem.splitScalar(u8, body, '\n');
    var checked: usize = 0;
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, "agentsfleet_library_")) continue;
        const open = std.mem.indexOfScalar(u8, line, '{') orelse continue;
        const close = std.mem.indexOfScalar(u8, line, '}') orelse continue;

        var pairs = std.mem.splitScalar(u8, line[open + 1 .. close], ',');
        while (pairs.next()) |pair| {
            const eq = std.mem.indexOfScalar(u8, pair, '=') orelse return error.MalformedLabel;
            const key = pair[0..eq];
            const value = std.mem.trim(u8, pair[eq + 1 ..], "\"");

            const permitted: []const []const u8 = if (std.mem.eql(u8, key, stages.LABEL_SURFACE))
                &stages.SURFACE_LABELS
            else if (std.mem.eql(u8, key, stages.LABEL_STAGE))
                &stages.STAGE_LABELS
            else if (std.mem.eql(u8, key, stages.LABEL_OUTCOME))
                &stages.OUTCOME_LABELS
            else if (std.mem.eql(u8, key, stages.LABEL_POOL_RESULT))
                &stages.POOL_RESULT_LABELS
            else if (std.mem.eql(u8, key, stages.LABEL_CACHE))
                &stages.CACHE_LABELS
            else
                return error.UnpermittedLabelKey;

            var found = false;
            for (permitted) |allowed| {
                if (std.mem.eql(u8, allowed, value)) found = true;
            }
            try testing.expect(found);
            checked += 1;
        }
    }

    // A pass with nothing checked would be vacuous — it is what this test looks
    // like if the families stop rendering entirely.
    try testing.expect(checked > 0);
}

// ── concurrency: no lost increments, no hidden serialization ────────────────

const CONCURRENT_WRITERS: usize = 128;
const OBSERVATIONS_PER_WRITER: usize = 200;

const Writer = struct {
    start: *std.atomic.Value(bool),

    fn run(self: *Writer) void {
        // Barrier: every thread spins until released together, so the writes
        // genuinely contend instead of being staggered by spawn latency. A
        // staggered test can pass on a recorder that loses increments under
        // real contention.
        while (!self.start.load(.acquire)) std.atomic.spinLoopHint();

        var i: usize = 0;
        while (i < OBSERVATIONS_PER_WRITER) : (i += 1) {
            stages.observeStage(.{
                .surface = .tenant_models,
                .stage = .sql,
                .outcome = .ok,
                .duration_ns = 1,
                .bytes = 1,
                .count = 1,
            });
            stages.observeReadOutcome(.tenant_models, .ok);
        }
    }
};

// Every increment survives 128 threads writing the same cells at once.
//
// The families are lock-free `fetchAdd`, which is exactly the shape where a
// mistaken read-modify-write (load, add, store) passes every single-threaded
// test and silently drops counts in production under load. An exact total is
// the only assertion that catches it — "greater than zero" would not.
test "should not lose a single increment when 128 threads write the same cells" {
    stages.resetForTest();
    defer stages.resetForTest();

    var start = std.atomic.Value(bool).init(false);
    var writers: [CONCURRENT_WRITERS]Writer = undefined;
    for (&writers) |*w| w.* = .{ .start = &start };

    var threads: [CONCURRENT_WRITERS]std.Thread = undefined;
    for (&threads, &writers) |*t, *w| t.* = try std.Thread.spawn(.{}, Writer.run, .{w});
    start.store(true, .release);
    for (&threads) |*t| t.join();

    const expected: u64 = CONCURRENT_WRITERS * OBSERVATIONS_PER_WRITER;
    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);

    try testing.expectEqual(expected, snap.stages[s][@intFromEnum(stages.Stage.sql)].count);
    try testing.expectEqual(expected, snap.stages[s][@intFromEnum(stages.Stage.sql)].duration_ns);
    try testing.expectEqual(expected, snap.read_outcomes[s][@intFromEnum(stages.Outcome.ok)]);
    try testing.expectEqual(expected, snap.payload_bytes[s]);
    try testing.expectEqual(expected, snap.results[s]);
}

// Contention must not bleed across cells. A recorder that indexed with a shared
// mutable cursor rather than by enum value would pass the totals above while
// writing some increments into a neighbouring stage.
test "should confine concurrent writes to the cells they name" {
    stages.resetForTest();
    defer stages.resetForTest();

    var start = std.atomic.Value(bool).init(false);
    var writers: [CONCURRENT_WRITERS]Writer = undefined;
    for (&writers) |*w| w.* = .{ .start = &start };

    var threads: [CONCURRENT_WRITERS]std.Thread = undefined;
    for (&threads, &writers) |*t, *w| t.* = try std.Thread.spawn(.{}, Writer.run, .{w});
    start.store(true, .release);
    for (&threads) |*t| t.join();

    const snap = stages.snapshot();
    const s = @intFromEnum(stages.Surface.tenant_models);
    // Every stage other than the one written stays exactly zero.
    inline for (@typeInfo(stages.Stage).@"enum".fields) |field| {
        if (field.value != @intFromEnum(stages.Stage.sql)) {
            try testing.expectEqual(@as(u64, 0), snap.stages[s][field.value].count);
        }
    }
    // And no other surface moved at all.
    for ([_]stages.Surface{ .global_models, .fleet_summary }) |other| {
        const o = @intFromEnum(other);
        for (snap.stages[o]) |cell| try testing.expectEqual(@as(u64, 0), cell.count);
        for (snap.read_outcomes[o]) |v| try testing.expectEqual(@as(u64, 0), v);
    }
}
