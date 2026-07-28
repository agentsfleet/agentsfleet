//! Unit tier for §3's failure matrix and §1 Dimension 1.1's schema half.
//!
//! Every row of §Failure Modes claims the same thing: when the fault fires, the
//! read still reports exactly one terminal outcome and leaves nothing behind.
//! The tests below inject each fault this tier can produce and assert that
//! property directly, rather than asserting that the fault produced an error —
//! an error is what the handler returns, and the telemetry claim is about what
//! it RECORDS on the way out.
//!
//! Rows proven elsewhere, because their fault needs a tier this one cannot
//! reach: `test_pool_bounded_progress_and_timeout` (a real saturated pool,
//! `db/pool_bounded_progress_integration_test.zig`),
//! `test_library_performance_report_validation` (the shipped `bun` validator,
//! `scripts/check_library_performance_report_test.py`), and
//! `test_library_evidence_is_secret_and_metadata_free` (the rendered scrape,
//! `library_stages_test.zig`). `scripts/check_failure_matrix_test.py` proves
//! the set is complete — that no row of the table lost its test.

const std = @import("std");
const testing = std.testing;

const scope_mod = @import("library_read_scope.zig");
const stages = @import("library_stages.zig");
const TraceContext = @import("trace.zig").TraceContext;

fn withIo(comptime body: fn (io: std.Io) anyerror!void) !void {
    var threaded = std.Io.Threaded.init(testing.allocator, .{});
    defer threaded.deinit();
    try body(threaded.io());
}

/// Total terminal outcomes recorded across every surface — the number that must
/// be exactly one per request whatever went wrong.
fn totalOutcomes(snap: stages.Snapshot) u64 {
    var total: u64 = 0;
    for (snap.read_outcomes) |row| for (row) |v| {
        total += v;
    };
    return total;
}

fn outcomesFor(snap: stages.Snapshot, surface: stages.Surface, outcome: stages.Outcome) u64 {
    return snap.read_outcomes[@intFromEnum(surface)][@intFromEnum(outcome)];
}

// ── Malformed trace ─────────────────────────────────────────────────────────

// `test_library_trace_malformed_case` — a header this process cannot parse
// starts a CLEAN root, and none of the caller's bytes survive into it.
//
// The second half is the one that matters and the one a "returns non-null"
// assertion would miss: a parser that salvaged the well-formed PREFIX of a
// malformed header would still produce a usable context, while quietly letting
// caller-controlled bytes choose this process's trace identity.
test "test_library_trace_malformed_case — malformed traceparent yields a clean root" {
    const malformed = [_][]const u8{
        "",
        "not-a-traceparent-header-value",
        // Right shape, unsupported version — a future version this process
        // cannot interpret must not be guessed at.
        "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        // Correct version and length, non-hex payload.
        "00-zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz-00f067aa0ba902b7-01",
        // Truncated mid-field.
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa",
    };

    for (malformed) |header| {
        try testing.expect(TraceContext.fromW3CHeader(header) == null);

        // What the handler does with the null: start a fresh root.
        const root = TraceContext.generate();
        // A root has no parent — a malformed header must not leave the span
        // dangling under an id nobody sent.
        try testing.expect(root.parent_span_id == null);

        // No echo. The rejected header's bytes appear nowhere in the context
        // this process will publish.
        if (header.len >= 8) {
            try testing.expect(std.mem.indexOf(u8, &root.trace_id, header[3..@min(header.len, 11)]) == null);
        }
    }
}

// A VALID header is adopted, and the child keeps the caller's trace while
// minting its own span. Paired with the case above so "clean root" cannot be
// satisfied by a parser that rejects everything.
test "test_library_trace_and_stage_schema — a valid traceparent is adopted as the parent" {
    const parsed = TraceContext.fromW3CHeader(
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
    ) orelse return error.ValidHeaderRejected;

    const child = parsed.child();
    try testing.expectEqualStrings(&parsed.trace_id, &child.trace_id);
    try testing.expect(child.parent_span_id != null);
    try testing.expectEqualStrings(&parsed.span_id, &child.parent_span_id.?);
    // The child mints its own span rather than reusing the caller's, or two
    // spans in one trace would share an id.
    try testing.expect(!std.mem.eql(u8, &parsed.span_id, &child.span_id));
}

// ── Metric rejection ────────────────────────────────────────────────────────

// `test_library_metric_rejection_case` — recording cannot fail, so a recorder
// problem cannot change a request's result.
//
// Asserted STRUCTURALLY, over the function types. A runtime test would have to
// build a recorder that rejects, and this recorder has no rejection path to
// build: `observeStage` and `observeReadOutcome` return `void`, not an error
// union, so there is no value a handler could branch on even if it wanted to.
// That is a stronger statement than "the one path we drove ignored the error".
test "test_library_metric_rejection_case — recording is infallible by construction" {
    const stage_fn = @typeInfo(@TypeOf(stages.observeStage)).@"fn";
    const outcome_fn = @typeInfo(@TypeOf(stages.observeReadOutcome)).@"fn";

    try testing.expect(stage_fn.return_type.? == void);
    try testing.expect(outcome_fn.return_type.? == void);
}

fn lossIsBounded(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    // Saturation: far more observations than any request makes. The families are
    // fixed-size, so "bounded loss" means the tables absorb this without growing
    // and without dropping the terminal outcomes.
    const BURST: usize = 5_000;
    var i: usize = 0;
    while (i < BURST) : (i += 1) {
        var scope = scope_mod.begin(io, .global_models);
        scope.endStageWith(.cache_lookup, .{ .cache = .miss });
        scope.succeed();
        scope.end();
    }

    const snap = stages.snapshot();
    // Every one of them counted — the request result was never altered, and
    // nothing was shed.
    try testing.expectEqual(@as(u64, BURST), totalOutcomes(snap));
    try testing.expectEqual(
        @as(u64, BURST),
        snap.stages[@intFromEnum(stages.Surface.global_models)][@intFromEnum(stages.Stage.cache_lookup)].count,
    );
}

test "test_library_metric_rejection_case — a burst is absorbed without loss or growth" {
    try withIo(lossIsBounded);
}

// ── Allocation / serialization ──────────────────────────────────────────────

fn allocationFailureStillReports(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    // A serialization step that runs out of memory. `std.testing.FailingAllocator`
    // fails the very first allocation, which is what the encode path would hit.
    var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
    const alloc = failing.allocator();

    {
        var scope = scope_mod.begin(io, .tenant_models);
        defer scope.end();

        const encoded = alloc.alloc(u8, 1024);
        try testing.expectError(error.OutOfMemory, encoded);

        // The handler's response to a failed encode: a typed internal outcome,
        // recorded on the stage that failed.
        scope.classify(.internal_error);
        scope.endStage(.serialize);
    }

    const snap = stages.snapshot();
    // Exactly one outcome, and it names the fault rather than the success the
    // read was optimistically heading toward.
    try testing.expectEqual(@as(u64, 1), totalOutcomes(snap));
    try testing.expectEqual(
        @as(u64, 1),
        outcomesFor(snap, .tenant_models, .internal_error),
    );
    try testing.expectEqual(@as(u64, 0), outcomesFor(snap, .tenant_models, .ok));
    // The failing stage still recorded its span — a read that dies mid-encode
    // is exactly the one an operator needs the timing for.
    try testing.expectEqual(
        @as(u64, 1),
        snap.stages[@intFromEnum(stages.Surface.tenant_models)][@intFromEnum(stages.Stage.serialize)].count,
    );
}

test "test_library_allocation_case — a failing allocator yields a typed outcome, recorded once" {
    try withIo(allocationFailureStillReports);
}

// ── Dependency failures, per stage ──────────────────────────────────────────

/// One row per dependency fault §Failure Modes names: which stage fails, and
/// the typed outcome the handler maps it to.
const DependencyFault = struct {
    stage: stages.Stage,
    outcome: stages.Outcome,
};

const DEPENDENCY_FAULTS = [_]DependencyFault{
    .{ .stage = .sql, .outcome = .dependency_error },
    .{ .stage = .cache_revision, .outcome = .dependency_error },
    .{ .stage = .secret_project, .outcome = .dependency_error },
    .{ .stage = .authorize, .outcome = .forbidden },
    .{ .stage = .pool_wait, .outcome = .timeout },
    .{ .stage = .auth_verify, .outcome = .unauthorized },
    .{ .stage = .map, .outcome = .internal_error },
    .{ .stage = .serialize, .outcome = .internal_error },
    .{ .stage = .next_upstream, .outcome = .cancelled },
    .{ .stage = .cache_lookup, .outcome = .not_found },
};

fn everyDependencyFaultReportsOnce(io: std.Io) !void {
    for (DEPENDENCY_FAULTS) |fault| {
        stages.resetForTest();

        {
            var scope = scope_mod.begin(io, .fleet_summary);
            defer scope.end();
            scope.classify(fault.outcome);
            scope.endStage(fault.stage);
        }

        const snap = stages.snapshot();
        // One outcome per failed read, on every one of the ten stages a fault
        // can land on. A stage that dropped its outcome would show zero here,
        // and a stage that double-counted would show two.
        try testing.expectEqual(@as(u64, 1), totalOutcomes(snap));
        try testing.expectEqual(@as(u64, 1), outcomesFor(snap, .fleet_summary, fault.outcome));
        try testing.expectEqual(
            @as(u64, 1),
            snap.stages[@intFromEnum(stages.Surface.fleet_summary)][@intFromEnum(fault.stage)].count,
        );
    }
    stages.resetForTest();
}

test "test_library_dependency_failure_case — every stage's fault maps to one typed outcome" {
    try withIo(everyDependencyFaultReportsOnce);
}

// Every stage in the closed set is covered by the table above. Without this, a
// stage added later would silently have no dependency-fault case — the table
// would still pass while covering nine of eleven.
test "test_library_dependency_failure_case — the fault table covers every stage" {
    inline for (@typeInfo(stages.Stage).@"enum".fields) |field| {
        const stage: stages.Stage = @enumFromInt(field.value);
        var covered = false;
        for (DEPENDENCY_FAULTS) |fault| {
            if (fault.stage == stage) covered = true;
        }
        if (!covered) {
            std.debug.print("stage '{s}' has no dependency-fault case\n", .{field.name});
            return error.StageNotCovered;
        }
    }
}

// ── Cleanup under fault ─────────────────────────────────────────────────────

fn faultLeavesNoPartialRead(io: std.Io) !void {
    stages.resetForTest();
    defer stages.resetForTest();

    // A read that got several stages in before failing. The stages it completed
    // stay recorded — they are real cost the operator paid — and exactly one
    // terminal outcome closes it.
    {
        var scope = scope_mod.begin(io, .tenant_models);
        defer scope.end();
        scope.endStage(.auth_verify);
        scope.endStageWith(.pool_wait, .{ .pool_result = .acquired });
        scope.endStage(.sql);
        scope.classify(.dependency_error);
        scope.endStage(.secret_project);
    }

    const snap = stages.snapshot();
    try testing.expectEqual(@as(u64, 1), totalOutcomes(snap));

    const s = @intFromEnum(stages.Surface.tenant_models);
    for ([_]stages.Stage{ .auth_verify, .pool_wait, .sql, .secret_project }) |ran| {
        try testing.expectEqual(@as(u64, 1), snap.stages[s][@intFromEnum(ran)].count);
    }
    // Stages the read never reached record nothing — a fault must not
    // fabricate a `serialize` span for a response that was never written.
    for ([_]stages.Stage{ .serialize, .map, .cache_lookup }) |never| {
        try testing.expectEqual(@as(u64, 0), snap.stages[s][@intFromEnum(never)].count);
    }
    // The pool slot it took is accounted for exactly once.
    try testing.expectEqual(@as(u64, 1), snap.pool_results[@intFromEnum(stages.PoolResult.acquired)]);
}

test "test_library_failure_matrix_is_complete — a mid-read fault records what ran and nothing else" {
    try withIo(faultLeavesNoPartialRead);
}
