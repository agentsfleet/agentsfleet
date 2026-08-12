//! Exported-window coverage for the library read-evidence families — the wire
//! half of library_stages_test.zig (structural + concurrency claims stay
//! there). Registered from tests.zig.

const std = @import("std");
const testing = std.testing;

const stages = @import("library_stages.zig");
const window = @import("otel_metrics_window_test.zig");

// The exported window carries every family, and — the half that matters for §1
// Dimension 1.2 — carries nothing else. Scanning the WHOLE flush body rather
// than one family is deliberate: a leak reaches an operator through whatever
// series happens to carry it, so the assertion has to read the same bytes the
// exporter ships.
test "test_library_evidence_is_secret_and_metadata_free — the export emits closed labels only" {
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
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // Every family is present.
    try window.expectFamilySample(body, stages.STAGE_DURATION_NAME);
    try window.expectFamilySample(body, stages.STAGE_OBSERVATIONS_NAME);
    try window.expectFamilySample(body, stages.READ_OUTCOME_NAME);
    try window.expectFamilySample(body, stages.POOL_RESULT_NAME);
    try window.expectFamilySample(body, stages.CACHE_OUTCOME_NAME);
    try window.expectFamilySample(body, stages.PAYLOAD_BYTES_NAME);
    try window.expectFamilySample(body, stages.RESULTS_NAME);

    // One second recorded exports as one second, so the ns->s conversion is
    // not silently dropping or scaling the measurement (exact decimal, no
    // float arithmetic).
    var surface_buf: [96]u8 = undefined;
    var stage_buf: [96]u8 = undefined;
    try window.expectFamilyWith(body, stages.STAGE_DURATION_NAME, &.{
        try window.attrFragment(&surface_buf, stages.LABEL_SURFACE, "tenant_models"),
        try window.attrFragment(&stage_buf, stages.LABEL_STAGE, "secret_project"),
        "\"asDouble\":1.000000000", // pin test: literal is the contract
    });

    // Every label on every library series is checked against an ALLOW list, not
    // a deny list. A deny list of secret-shaped substrings cannot decide this
    // surface: `stage="sql"` and `stage="secret_project"` are legitimate closed
    // values that contain "sql" and "secret", so a substring scan either
    // false-fires on them or is weakened until it catches nothing. Requiring
    // each key to be one of §1's five and each value to be a member of that
    // key's enum is the property §1 actually states, and it rejects a
    // free-form value no deny list would have thought to spell.
    var checked: usize = 0;
    var cursor: usize = 0;
    const LIBRARY_NAME_MARK = "\"name\":\"agentsfleet_library_";
    const ATTRS_OPEN = "\"attributes\":[";
    while (std.mem.indexOfPos(u8, body, cursor, LIBRARY_NAME_MARK)) |obj_start| {
        const attrs_start = (std.mem.indexOfPos(u8, body, obj_start, ATTRS_OPEN) orelse return error.MalformedObject) + ATTRS_OPEN.len;
        const attrs_end = std.mem.indexOfScalarPos(u8, body, attrs_start, ']') orelse return error.MalformedObject;
        var pair_cursor: usize = attrs_start;
        while (std.mem.indexOfPos(u8, body[0..attrs_end], pair_cursor, "\"key\":\"")) |key_at| {
            const key_start = key_at + "\"key\":\"".len;
            const key_end = std.mem.indexOfScalarPos(u8, body, key_start, '"') orelse return error.MalformedLabel;
            const key = body[key_start..key_end];
            const VALUE_MARK = "\"stringValue\":\"";
            const val_at = (std.mem.indexOfPos(u8, body[0..attrs_end], key_end, VALUE_MARK) orelse return error.MalformedLabel) + VALUE_MARK.len;
            const val_end = std.mem.indexOfScalarPos(u8, body, val_at, '"') orelse return error.MalformedLabel;
            const value = body[val_at..val_end];

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
            pair_cursor = val_end;
        }
        cursor = obj_start + LIBRARY_NAME_MARK.len;
    }

    // A pass with nothing checked would be vacuous — it is what this test looks
    // like if the families stop exporting entirely.
    try testing.expect(checked > 0);
}
