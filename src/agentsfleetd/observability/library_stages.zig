//! Closed telemetry schema for the authenticated library read paths.
//!
//! Every label this module can emit is an enum member declared here. There is no
//! entry point that accepts a string, so a caller cannot widen the label space
//! without editing this file — which is the property §1 asks for ("permit only
//! these enums and numeric duration/count/bytes") expressed as a type rather than
//! as a review convention.
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
//! a fixed aggregate") and this workstream's Invariant 1. The pool and cache
//! families deliberately carry NO surface label: §Metrics & Observability grants
//! them a closed outcome and nothing else, and a starving pool is a process-wide
//! condition rather than a per-surface one.
//!
//! ## Why stages are metrics and not spans
//!
//! `http/route_trace.zig` admits at most ten generic request spans per monotonic
//! second. Ten stage spans on one library request would spend a whole second's
//! admission budget and evict the server-error spans that budget exists to
//! protect. The trace half of §1 is therefore the ingress context — W3C
//! `traceparent` in, one `http.request` span out — which `handlers/common.zig`
//! already implements; stage timings live here.

const std = @import("std");

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
/// the label and what §1 names.
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
// is a shape `promtool check metrics` accepts but no operator can read, and the
// pair below states plainly what each number is. Mean stage cost is
// rate(duration_total) / rate(observations_total).
pub const STAGE_DURATION_NAME = "agentsfleet_library_stage_duration_seconds_total";
pub const STAGE_DURATION_HELP = "Seconds spent in one library read stage, by surface and stage. Divide by the observations counter for mean stage cost.";
pub const STAGE_OBSERVATIONS_NAME = "agentsfleet_library_stage_observations_total";
pub const STAGE_OBSERVATIONS_HELP = "Completed library read stages, by surface and stage. The denominator for the duration counter above.";
pub const READ_OUTCOME_NAME = "agentsfleet_library_read_outcome_total";
pub const READ_OUTCOME_HELP = "Library reads by surface and terminal outcome. Incremented exactly once per request.";
pub const POOL_RESULT_NAME = "agentsfleet_library_pool_result_total";
pub const POOL_RESULT_HELP = "Pool acquisitions on library read paths by result. Unlabelled by surface: a starving pool is process-wide, and a tenant label here would outlive the process guard.";
pub const CACHE_OUTCOME_NAME = "agentsfleet_library_cache_outcome_total";
pub const CACHE_OUTCOME_HELP = "Cache dispositions on library read paths. Global cache only; carries no tenant or request identity.";
pub const PAYLOAD_BYTES_NAME = "agentsfleet_library_payload_bytes_total";
pub const PAYLOAD_BYTES_HELP = "Encoded response bytes produced by library reads, by surface.";
pub const RESULTS_NAME = "agentsfleet_library_results_total";
pub const RESULTS_HELP = "Rows materialised into library read projections, by surface.";

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

// ── Family storage ──────────────────────────────────────────────────────────
//
// safe because: each cell is an independent monotonic counter. The /metrics
// scrape tolerates reading one family a few nanoseconds after another, and no
// other memory is published through these atomics.

const Counter = std.atomic.Value(u64);
const ZERO = Counter.init(0);

var g_stage_duration_ns: [N_SURFACES][N_STAGES]Counter = .{.{ZERO} ** N_STAGES} ** N_SURFACES;
var g_stage_count: [N_SURFACES][N_STAGES]Counter = .{.{ZERO} ** N_STAGES} ** N_SURFACES;
var g_read_outcome: [N_SURFACES][N_OUTCOMES]Counter = .{.{ZERO} ** N_OUTCOMES} ** N_SURFACES;
var g_pool_result: [N_POOL_RESULTS]Counter = .{ZERO} ** N_POOL_RESULTS;
var g_cache_outcome: [N_CACHE]Counter = .{ZERO} ** N_CACHE;
var g_payload_bytes: [N_SURFACES]Counter = .{ZERO} ** N_SURFACES;
var g_results: [N_SURFACES]Counter = .{ZERO} ** N_SURFACES;

fn idx(value: anytype) usize {
    return @intFromEnum(value);
}

/// Record one completed stage.
///
/// Feeds the stage-duration family always, and the pool and cache families only
/// when the observation states those dimensions. It deliberately does NOT touch
/// the read-outcome family: that one is per-request, and incrementing it here
/// would multiply every read by however many stages it happened to run.
pub fn observeStage(obs: LibraryObservation) void {
    const s = idx(obs.surface);
    _ = g_stage_duration_ns[s][idx(obs.stage)].fetchAdd(obs.duration_ns, .monotonic);
    _ = g_stage_count[s][idx(obs.stage)].fetchAdd(1, .monotonic);

    if (obs.pool_result) |result| {
        _ = g_pool_result[idx(result)].fetchAdd(1, .monotonic);
    }
    if (obs.cache != .not_applicable) {
        _ = g_cache_outcome[idx(obs.cache)].fetchAdd(1, .monotonic);
    }
    if (obs.bytes) |b| {
        _ = g_payload_bytes[s].fetchAdd(b, .monotonic);
    }
    if (obs.count) |c| {
        _ = g_results[s].fetchAdd(c, .monotonic);
    }
}

/// Record how one library read terminated. Called exactly once per request, on
/// every exit path including the failing ones — an absent sample would make a
/// read that died in `auth_verify` indistinguishable from one that never
/// arrived, which is the confusion this family exists to remove.
pub fn observeReadOutcome(surface: Surface, outcome: Outcome) void {
    _ = g_read_outcome[idx(surface)][idx(outcome)].fetchAdd(1, .monotonic);
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

/// Read every family for one scrape. Not atomic across families by design: the
/// exposition format has no cross-family consistency requirement, and taking a
/// lock on the scrape path would put the reader in the writers' way.
pub fn snapshot() Snapshot {
    // SAFETY: every field of `out` is assigned by the loops below — they are
    // bounded by the same enum field counts that size the struct — before it is
    // read or returned.
    var out: Snapshot = undefined;
    for (0..N_SURFACES) |s| {
        for (0..N_STAGES) |st| {
            out.stages[s][st] = .{
                .surface = @enumFromInt(s),
                .stage = @enumFromInt(st),
                .duration_ns = g_stage_duration_ns[s][st].load(.acquire),
                .count = g_stage_count[s][st].load(.acquire),
            };
        }
        for (0..N_OUTCOMES) |o| out.read_outcomes[s][o] = g_read_outcome[s][o].load(.acquire);
        out.payload_bytes[s] = g_payload_bytes[s].load(.acquire);
        out.results[s] = g_results[s].load(.acquire);
    }
    for (0..N_POOL_RESULTS) |p| out.pool_results[p] = g_pool_result[p].load(.acquire);
    for (0..N_CACHE) |c| out.cache_outcomes[c] = g_cache_outcome[c].load(.acquire);
    return out;
}

/// Zero every family so one test's assertion is not another test's total.
pub fn resetForTest() void {
    for (0..N_SURFACES) |s| {
        for (0..N_STAGES) |st| {
            g_stage_duration_ns[s][st].store(0, .release);
            g_stage_count[s][st].store(0, .release);
        }
        for (0..N_OUTCOMES) |o| g_read_outcome[s][o].store(0, .release);
        g_payload_bytes[s].store(0, .release);
        g_results[s].store(0, .release);
    }
    for (0..N_POOL_RESULTS) |p| g_pool_result[p].store(0, .release);
    for (0..N_CACHE) |c| g_cache_outcome[c].store(0, .release);
}

test {
    _ = @import("library_stages_test.zig");
}
