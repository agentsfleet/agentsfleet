//! Per-request lifecycle for library read telemetry.
//!
//! One `ReadScope` covers one authenticated library read: it times the stages
//! as they close and emits exactly one terminal outcome when it ends.
//!
//! ## Why a scope rather than calls at each exit
//!
//! `tenant_model_entries_list.zig` alone has nine ways to leave the handler —
//! missing tenant, unreadable query, out-of-range limit, two distinct cursor
//! rejections, pool exhaustion, a failed build, an over-ceiling body, and a
//! failed write — before counting the success path. Placing an
//! `observeReadOutcome` call at each one produces a per-request counter that
//! records the exits its author remembered, which is the exact failure mode
//! `library_read_counters.zig` documents for hand-placed statement tallies. The
//! tenth exit, added next quarter, silently reports nothing.
//!
//! A scope inverts that. `defer scope.end()` fires on every path by
//! construction, so a new exit is instrumented before it is written. The
//! default outcome is `internal_error` rather than `ok` for the same reason: a
//! path nobody classified must show up as something an operator investigates,
//! not as a success.
//!
//! ## Why the stage marker moves
//!
//! Stages are consecutive spans of one request, not nested ones, so the scope
//! keeps a single marker and `endStage` closes the span since the previous
//! marker before resetting it. That makes the recorded stage durations sum to
//! the request's own duration, which is what lets an operator read the stage
//! table as an attribution of the total rather than as unrelated samples.

const ReadScope = @This();

io: std.Io,
surface: stages.Surface,
/// Terminal outcome. Deliberately pessimistic until classified — see the note.
outcome: stages.Outcome = .internal_error,
/// Monotonic nanoseconds at the previous stage boundary.
stage_started_ns: i96,
/// Guards against a double `end()`; the counter must move once per request.
ended: bool = false,

/// Optional dimensions a stage may carry. A stage that has none passes `.{}`
/// and records only its duration.
pub const StageDetail = struct {
    cache: stages.Cache = .not_applicable,
    pool_result: ?stages.PoolResult = null,
    count: ?u64 = null,
    bytes: ?u64 = null,
};

pub fn begin(io: std.Io, surface: stages.Surface) ReadScope {
    return .{
        .io = io,
        .surface = surface,
        .stage_started_ns = std.Io.Clock.boot.now(io).toNanoseconds(),
    };
}

fn elapsedNs(self: *ReadScope) u64 {
    const now = std.Io.Clock.boot.now(self.io).toNanoseconds();
    const delta = now - self.stage_started_ns;
    self.stage_started_ns = now;
    // A monotonic clock cannot run backwards, but the subtraction is signed and
    // a negative would wrap to an enormous duration that poisons the sum for
    // the life of the process. Clamping costs one branch on a path that is
    // already doing a syscall-backed clock read.
    return if (delta < 0) 0 else @intCast(delta);
}

/// Close the current stage span and record it.
pub fn endStage(self: *ReadScope, stage: stages.Stage) void {
    self.endStageWith(stage, .{});
}

/// Close the current stage span and record it with its extra dimensions.
pub fn endStageWith(self: *ReadScope, stage: stages.Stage, detail: StageDetail) void {
    stages.observeStage(.{
        .surface = self.surface,
        .stage = stage,
        .outcome = self.outcome,
        .cache = detail.cache,
        .pool_result = detail.pool_result,
        .duration_ns = self.elapsedNs(),
        .count = detail.count,
        .bytes = detail.bytes,
    });
}

/// State how this read is ending. The LAST classification wins, so a handler
/// may mark success optimistically and then reclassify on a late failure such
/// as an over-ceiling body.
pub fn classify(self: *ReadScope, outcome: stages.Outcome) void {
    self.outcome = outcome;
}

pub fn succeed(self: *ReadScope) void {
    self.outcome = .ok;
}

/// Emit the terminal outcome. Idempotent: a handler that ends explicitly and
/// also carries the customary `defer` must still count once.
pub fn end(self: *ReadScope) void {
    if (self.ended) return;
    self.ended = true;
    stages.observeReadOutcome(self.surface, self.outcome);
}

const std = @import("std");
const stages = @import("library_stages.zig");

test {
    _ = @import("library_read_scope_test.zig");
}
