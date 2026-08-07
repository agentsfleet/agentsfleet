//! Scoring is where the benchmark's honesty is enforced: it refuses a violated
//! calibration/evaluation split, refuses a drifted or evaluation-tuned
//! baseline, and credits a detection only when the claim is structured —
//! service and class named within tolerance, citing an ES|QL digest.
//! "Anomaly found" scores zero.

const std = @import("std");
const manifest = @import("manifest.zig");
const baseline = @import("baseline.zig");
const report = @import("report.zig");

/// Slack after a seed window closes during which a detection still scores.
const DETECT_TOLERANCE_MS: i64 = 60_000;
const PCT_MAX: u32 = 100;
const P95_NUMERATOR: usize = 95;
/// Reason string carried on every threshold-win row (see baseline.thresholdCatches).
const THRESHOLD_WIN_REASON = "obvious_spike_over_error_rate_threshold";

/// One structured claim from a sweep run. This is the wire shape, so every
/// field is optional — scoring, not parsing, decides what counts.
pub const Finding = struct {
    service: ?[]const u8 = null,
    class: ?manifest.IncidentClass = null,
    esql_digest: ?[]const u8 = null,
    detected_offset_ms: ?i64 = null,
    actionable_offset_ms: ?i64 = null,
};

pub const Run = struct {
    findings: []const Finding = &.{},
    query_cost_usd_micros: u64 = 0,
    model_cost_usd_micros: u64 = 0,
};

/// A findings file as produced by a sweep: which corpus it ran against, and
/// one entry per repeated run.
pub const RunSet = struct {
    corpus_hash: []const u8,
    runs: []const Run,
};

/// Caller must free via `.deinit()` on the returned parse.
pub fn parseRunSet(alloc: std.mem.Allocator, raw: []const u8) !std.json.Parsed(RunSet) {
    return std.json.parseFromSlice(RunSet, alloc, raw, .{ .ignore_unknown_fields = true });
}

pub const ScoreError = manifest.DisjointError || baseline.FreezeError || std.mem.Allocator.Error;

/// Score repeated runs against the evaluation manifest. Caller must free the
/// returned report via `Report.deinit`.
pub fn score(
    alloc: std.mem.Allocator,
    eval_m: manifest.SeedManifest,
    calib_m: manifest.SeedManifest,
    b: baseline.Baseline,
    freeze: baseline.Freeze,
    runs: []const Run,
) ScoreError!report.Report {
    try manifest.assertDisjoint(eval_m, calib_m);
    try baseline.verifyFrozen(b, freeze);

    var ttds: std.ArrayList(i64) = .empty;
    defer ttds.deinit(alloc);
    var ttars: std.ArrayList(i64) = .empty;
    defer ttars.deinit(alloc);

    var total_incidents: u32 = 0;
    var detected_incidents: u32 = 0;
    var variance_sum: i64 = 0;
    var variance_groups: u32 = 0;
    for (eval_m.seeds) |seed| {
        if (seed.clean) continue;
        total_incidents += 1;
        var per_run_ttds: std.ArrayList(i64) = .empty;
        defer per_run_ttds.deinit(alloc);
        for (runs) |run| {
            const hit = findDetection(run, seed) orelse continue;
            const ttd = hit.detected_offset_ms.? - seed.offset_ms;
            try ttds.append(alloc, ttd);
            try per_run_ttds.append(alloc, ttd);
            if (hit.actionable_offset_ms) |a| try ttars.append(alloc, a - seed.offset_ms);
        }
        // Detected in any run counts toward recall; the variance row is what
        // exposes run-to-run flakiness rather than a stricter recall rule.
        if (per_run_ttds.items.len > 0) detected_incidents += 1;
        if (per_run_ttds.items.len > 1) {
            variance_sum += sampleVariance(per_run_ttds.items);
            variance_groups += 1;
        }
    }

    var false_positives: u32 = 0;
    var query_cost: u64 = 0;
    var model_cost: u64 = 0;
    for (runs) |run| {
        query_cost += run.query_cost_usd_micros;
        model_cost += run.model_cost_usd_micros;
        for (run.findings) |f| {
            if (!matchesAnyIncident(eval_m, f)) false_positives += 1;
        }
    }

    const wins = try collectThresholdWins(alloc, eval_m, b);
    errdefer alloc.free(wins);

    std.mem.sort(i64, ttds.items, {}, std.sort.asc(i64));
    std.mem.sort(i64, ttars.items, {}, std.sort.asc(i64));
    return .{
        .total_incidents = total_incidents,
        .detected_incidents = detected_incidents,
        .recall_pct = if (total_incidents == 0) 0 else detected_incidents * PCT_MAX / total_incidents,
        .false_positives = false_positives,
        .ttd_median_ms = median(ttds.items),
        .ttd_p95_ms = percentile95(ttds.items),
        .ttar_median_ms = median(ttars.items),
        .ttd_variance_ms = if (variance_groups == 0) 0 else @divTrunc(variance_sum, variance_groups),
        .query_cost_usd_micros = query_cost,
        .model_cost_usd_micros = model_cost,
        .threshold_wins = wins,
    };
}

/// A finding scores against a seed only when fully structured: service and
/// class both named and matching, an ES|QL digest cited, and the detection
/// timestamp inside the seed window (plus tolerance).
fn matches(f: Finding, seed: manifest.Seed) bool {
    const service = f.service orelse return false;
    const class = f.class orelse return false;
    const digest = f.esql_digest orelse return false;
    const detected = f.detected_offset_ms orelse return false;
    if (digest.len == 0) return false;
    if (!std.mem.eql(u8, service, seed.service)) return false;
    if (class != seed.class) return false;
    return detected >= seed.offset_ms and
        detected <= seed.offset_ms + seed.duration_ms + DETECT_TOLERANCE_MS;
}

fn findDetection(run: Run, seed: manifest.Seed) ?Finding {
    for (run.findings) |f| {
        if (matches(f, seed)) return f;
    }
    return null;
}

fn matchesAnyIncident(m: manifest.SeedManifest, f: Finding) bool {
    for (m.seeds) |seed| {
        if (seed.clean) continue;
        if (matches(f, seed)) return true;
    }
    return false;
}

/// Caller must free the returned slice; seed ids inside are borrowed.
fn collectThresholdWins(
    alloc: std.mem.Allocator,
    m: manifest.SeedManifest,
    b: baseline.Baseline,
) ![]report.ThresholdWin {
    var wins: std.ArrayList(report.ThresholdWin) = .empty;
    defer wins.deinit(alloc);
    for (m.seeds) |seed| {
        if (!baseline.thresholdCatches(b, seed)) continue;
        try wins.append(alloc, .{
            .seed_id = seed.id,
            .class = seed.class,
            .reason = THRESHOLD_WIN_REASON,
        });
    }
    return wins.toOwnedSlice(alloc);
}

fn median(sorted: []const i64) i64 {
    if (sorted.len == 0) return 0;
    return sorted[sorted.len / 2];
}

fn percentile95(sorted: []const i64) i64 {
    if (sorted.len == 0) return 0;
    const idx = (sorted.len - 1) * P95_NUMERATOR / PCT_MAX;
    return sorted[idx];
}

fn sampleVariance(values: []const i64) i64 {
    var sum: i64 = 0;
    for (values) |v| sum += v;
    const mean = @divTrunc(sum, @as(i64, @intCast(values.len)));
    var sq_sum: i64 = 0;
    for (values) |v| sq_sum += (v - mean) * (v - mean);
    return @divTrunc(sq_sum, @as(i64, @intCast(values.len - 1)));
}
