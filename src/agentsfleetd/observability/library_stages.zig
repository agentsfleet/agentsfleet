//! Closed telemetry schema for the authenticated library read paths.
//!
//! Every label this module can emit is an enum member declared here. There is no
//! entry point that accepts a string, so a caller cannot widen the label space
//! without editing this file — "permit only these enums and numeric
//! duration/count/bytes" expressed as a type rather than as a review
//! convention. The registry (otel_metrics_families.zig) declares each family's
//! dimensions off these enums, and storage lives in the generated instrument
//! layer (otel_instruments.zig).
//!
//! ## Why the observation fans out instead of carrying five labels
//!
//! `LibraryObservation` names five closed dimensions. Emitting it as ONE metric
//! point labelled by all five is a cross-product: 3 surfaces x 10 stages x
//! 9 outcomes x 5 cache values x 4 pool results = 5400 series, most of them
//! permanently zero. So the observation is the INPUT shape and the families
//! below are the output shape — each takes only the dimensions that vary for it:
//!
//!   - stage duration       {surface, stage}   30 series x (sum, count)
//!   - read outcome         {surface, outcome} 27 series, once per REQUEST
//!   - pool result          {pool_result}       4 series
//!   - cache outcome        {cache}             5 series
//!   - payload bytes        {surface}           3 series
//!   - results              {surface}           3 series
//!
//! That is 102 fixed series, matching `docs/architecture/observability.md`
//! §Metrics ("a metric is justified only when it answers an operator question as
//! a fixed aggregate"). The pool and cache families deliberately carry NO
//! surface label: a starving pool is a process-wide condition rather than a
//! per-surface one.
//!
//! ## Why stages are metrics and not spans
//!
//! `http/route_trace.zig` admits at most ten generic request spans per monotonic
//! second. Ten stage spans on one library request would spend a whole second's
//! admission budget and evict the server-error spans that budget exists to
//! protect. The trace half is therefore the ingress context — W3C `traceparent`
//! in, one `http.request` span out — which `handlers/common.zig` already
//! implements; stage timings live here.

const std = @import("std");
const instruments = @import("otel_instruments.zig");

/// The authenticated read surfaces this schema can describe.
///
/// `fleet_detail` is deliberately ABSENT. That route was stripped unconsumed
/// — the handler, its matcher, and its registration are all gone — so
/// an enum member for it could never be emitted by any producer, and a dashboard
/// would render it as a permanently empty series (RULE NDC).
pub const Surface = enum { tenant_models, global_models, fleet_summary };

/// The stages one library read passes through. `secret_project` survives the
/// read-path decryption removal with a narrowed meaning: it now times presence
/// resolution and projection (`state/vault.zig` `loadMetadata`, one batch query,
/// zero decryptions) rather than per-row decryption. It is kept rather than
/// deleted so that reintroducing per-row decryption shows up as a stage that
/// suddenly decrypts, instead of as a stage that silently reappears.
pub const Stage = enum {
    next_upstream,
    auth_verify,
    pool_wait,
    authorize,
    sql,
    secret_project,
    map,
    serialize,
    cache_revision,
    cache_lookup,
};

/// How a read (or the stage that ended it) terminated.
pub const Outcome = enum {
    ok,
    invalid,
    unauthorized,
    forbidden,
    not_found,
    timeout,
    cancelled,
    dependency_error,
    internal_error,
};

/// Cache disposition. `not_applicable` is the default so a stage that never
/// consults a cache records no cache series rather than a misleading `miss`.
pub const Cache = enum { hit, miss, bypass, stale, not_applicable };

/// Outcome of one pool acquisition. `error` is a Zig keyword, so the member is
/// spelled `@"error"`; its `@tagName` is still `"error"`, which is what reaches
/// the label.
pub const PoolResult = enum { acquired, timeout, cancelled, @"error" };

/// One completed stage of one library read.
///
/// `cache` defaults to `not_applicable` and `pool_result` to null so that a
/// caller states only the dimensions its stage actually has. A stage that
/// supplies neither feeds exactly one family (stage duration), which is what
/// keeps the per-request series count independent of how many stages ran.
pub const LibraryObservation = struct {
    surface: Surface,
    stage: Stage,
    outcome: Outcome,
    cache: Cache = .not_applicable,
    pool_result: ?PoolResult = null,
    /// Nanoseconds, not seconds: the exposition divides once at render time, so
    /// the accumulator stays an integer and `fetchAdd` stays lock-free. A float
    /// sum would need a compare-and-swap loop for no gain in precision here.
    duration_ns: u64,
    /// Rows the stage materialised into the response projection, when it has any.
    count: ?u64 = null,
    /// Encoded response bytes the stage produced, when it produces a body.
    bytes: ?u64 = null,
};

// ── Family names and labels (RULE UFS: one home per wire string) ─────────────

// Two explicit counters rather than one `summary`: a summary with no quantiles
// is a shape no operator can read, and the pair below states plainly what each
// number is. Mean stage cost is rate(duration_total) / rate(observations_total).
pub const STAGE_DURATION_NAME = "agentsfleet_library_stage_duration_seconds_total";
pub const STAGE_OBSERVATIONS_NAME = "agentsfleet_library_stage_observations_total";
pub const READ_OUTCOME_NAME = "agentsfleet_library_read_outcome_total";
pub const POOL_RESULT_NAME = "agentsfleet_library_pool_result_total";
pub const CACHE_OUTCOME_NAME = "agentsfleet_library_cache_outcome_total";
pub const PAYLOAD_BYTES_NAME = "agentsfleet_library_payload_bytes_total";
pub const RESULTS_NAME = "agentsfleet_library_results_total";

pub const LABEL_SURFACE = "surface";
pub const LABEL_STAGE = "stage";
pub const LABEL_OUTCOME = "outcome";
pub const LABEL_POOL_RESULT = "pool_result";
pub const LABEL_CACHE = "cache";

fn labelsOf(comptime E: type) [@typeInfo(E).@"enum".fields.len][]const u8 {
    const fields = @typeInfo(E).@"enum".fields;
    var out: [fields.len][]const u8 = undefined;
    for (fields, 0..) |f, i| out[i] = f.name;
    return out;
}

const N_SURFACES = @typeInfo(Surface).@"enum".fields.len;
const N_STAGES = @typeInfo(Stage).@"enum".fields.len;
const N_OUTCOMES = @typeInfo(Outcome).@"enum".fields.len;
const N_CACHE = @typeInfo(Cache).@"enum".fields.len;
const N_POOL_RESULTS = @typeInfo(PoolResult).@"enum".fields.len;

pub const SURFACE_LABELS = labelsOf(Surface);
pub const STAGE_LABELS = labelsOf(Stage);
pub const OUTCOME_LABELS = labelsOf(Outcome);
pub const CACHE_LABELS = labelsOf(Cache);
pub const POOL_RESULT_LABELS = labelsOf(PoolResult);

/// Every series this module can ever emit, counted at compile time.
///
/// Asserted rather than merely documented: the number is the whole cardinality
/// argument, and adding one enum member silently multiplies it. A member added
/// without revisiting the budget fails the build here rather than showing up as
/// scrape growth in production.
pub const TOTAL_SERIES: usize =
    N_SURFACES * N_STAGES * 2 // duration sum + count
    + N_SURFACES * N_OUTCOMES // read outcome
    + N_POOL_RESULTS // pool result
    + N_CACHE // cache outcome
    + N_SURFACES * 2; // payload bytes + results

comptime {
    // pin test: literal is the contract — see the module note's series table.
    std.debug.assert(TOTAL_SERIES == 102);
}

/// Record one completed stage.
///
/// Feeds the stage-duration family always, and the pool and cache families only
/// when the observation states those dimensions. It deliberately does NOT touch
/// the read-outcome family: that one is per-request, and incrementing it here
/// would multiply every read by however many stages it happened to run.
pub fn observeStage(obs: LibraryObservation) void {
    instruments.add(.library_stage_duration, .{ .surface = obs.surface, .stage = obs.stage }, obs.duration_ns);
    instruments.inc(.library_stage_observations, .{ .surface = obs.surface, .stage = obs.stage });

    if (obs.pool_result) |result| {
        instruments.inc(.library_pool_result, .{ .pool_result = result });
    }
    if (obs.cache != .not_applicable) {
        instruments.inc(.library_cache_outcome, .{ .cache = obs.cache });
    }
    if (obs.bytes) |b| {
        instruments.add(.library_payload_bytes, .{ .surface = obs.surface }, b);
    }
    if (obs.count) |c| {
        instruments.add(.library_results, .{ .surface = obs.surface }, c);
    }
}

/// Record how one library read terminated. Called exactly once per request, on
/// every exit path including the failing ones — an absent sample would make a
/// read that died in `auth_verify` indistinguishable from one that never
/// arrived, which is the confusion this family exists to remove.
pub fn observeReadOutcome(surface: Surface, outcome: Outcome) void {
    instruments.inc(.library_read_outcome, .{ .surface = surface, .outcome = outcome });
}

pub const StageSample = struct {
    surface: Surface,
    stage: Stage,
    duration_ns: u64,
    count: u64,
};

pub const Snapshot = struct {
    stages: [N_SURFACES][N_STAGES]StageSample,
    read_outcomes: [N_SURFACES][N_OUTCOMES]u64,
    pool_results: [N_POOL_RESULTS]u64,
    cache_outcomes: [N_CACHE]u64,
    payload_bytes: [N_SURFACES]u64,
    results: [N_SURFACES]u64,
};

/// Read every family for one assertion window. Not atomic across families by
/// design: no cross-family consistency requirement exists, and a lock would
/// put the reader in the writers' way.
pub fn snapshot() Snapshot {
    // SAFETY: every field of `out` is assigned by the loops below — they are
    // bounded by the same enum field counts that size the struct — before it is
    // read or returned.
    var out: Snapshot = undefined;
    for (0..N_SURFACES) |s| {
        const surface: Surface = @enumFromInt(s);
        for (0..N_STAGES) |st| {
            const stage: Stage = @enumFromInt(st);
            out.stages[s][st] = .{
                .surface = surface,
                .stage = stage,
                .duration_ns = instruments.snapshotCell(.library_stage_duration, .{ .surface = surface, .stage = stage }),
                .count = instruments.snapshotCell(.library_stage_observations, .{ .surface = surface, .stage = stage }),
            };
        }
        for (0..N_OUTCOMES) |o| out.read_outcomes[s][o] = instruments.snapshotCell(.library_read_outcome, .{ .surface = surface, .outcome = @enumFromInt(o) });
        out.payload_bytes[s] = instruments.snapshotCell(.library_payload_bytes, .{ .surface = surface });
        out.results[s] = instruments.snapshotCell(.library_results, .{ .surface = surface });
    }
    for (0..N_POOL_RESULTS) |p| out.pool_results[p] = instruments.snapshotCell(.library_pool_result, .{ .pool_result = @enumFromInt(p) });
    for (0..N_CACHE) |c| out.cache_outcomes[c] = instruments.snapshotCell(.library_cache_outcome, .{ .cache = @enumFromInt(c) });
    return out;
}

/// Zero every family so one test's assertion is not another test's total.
pub fn resetForTest() void {
    instruments.resetCellsForTest(&.{
        .library_stage_duration,
        .library_stage_observations,
        .library_read_outcome,
        .library_pool_result,
        .library_cache_outcome,
        .library_payload_bytes,
        .library_results,
    });
}

test {
    _ = @import("library_stages_test.zig");
}
