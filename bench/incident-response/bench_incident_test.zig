//! Unit tests for the incident-response benchmark harness. Test names mirror
//! the acceptance rubric's grep targets — trust names in output, not exit
//! codes. Fixtures are struct literals: the harness's JSON parsing is thin
//! std.json, and the honesty properties live in the pure functions.

const std = @import("std");
const manifest = @import("manifest.zig");
const injector = @import("injector.zig");
const baseline = @import("baseline.zig");
const scoring = @import("scoring.zig");
const report = @import("report.zig");

const testing = std.testing;

// Fixed corpus epoch — the corpus derives every timestamp from this.
const EPOCH_MS: i64 = 1_754_000_000_000;
const SPIKE_OFFSET_MS: i64 = 600_000;
const SPIKE_DURATION_MS: i64 = 300_000;

const EVAL_SEEDS = [_]manifest.Seed{
    .{ .id = "eval-spike-01", .class = .obvious_spike, .service = "checkout", .offset_ms = SPIKE_OFFSET_MS, .duration_ms = SPIKE_DURATION_MS, .magnitude_pct = 40 },
    .{ .id = "eval-burn-01", .class = .slow_burn, .service = "billing", .offset_ms = 1_200_000, .duration_ms = 900_000, .magnitude_pct = 10 },
    .{ .id = "eval-trace-01", .class = .trace_failure, .service = "gateway", .offset_ms = 2_400_000, .duration_ms = 300_000, .magnitude_pct = 20 },
    .{ .id = "eval-clean-01", .class = .obvious_spike, .service = "worker", .offset_ms = 3_000_000, .duration_ms = 300_000, .magnitude_pct = 0, .clean = true },
};

const CALIB_SEEDS = [_]manifest.Seed{
    .{ .id = "calib-spike-01", .class = .obvious_spike, .service = "checkout", .offset_ms = 0, .duration_ms = 300_000, .magnitude_pct = 35 },
    .{ .id = "calib-clean-01", .class = .slow_burn, .service = "billing", .offset_ms = 600_000, .duration_ms = 300_000, .magnitude_pct = 0, .clean = true },
};

const BASELINE = baseline.Baseline{
    .error_rate_pct_max = 25,
    .latency_p95_ms_max = 400,
    .saturation_pct_max = 80,
    .multi_window_breaches_min = 3,
};

fn evalManifest() manifest.SeedManifest {
    return .{ .set = .evaluation, .epoch_ms = EPOCH_MS, .seeds = &EVAL_SEEDS };
}

fn calibManifest() manifest.SeedManifest {
    return .{ .set = .calibration, .epoch_ms = EPOCH_MS, .seeds = &CALIB_SEEDS };
}

// A fully structured finding that matches eval-spike-01: service, class,
// digest, and a detection timestamp inside the seed window.
const STRUCTURED_SPIKE_FINDING = scoring.Finding{
    .service = "checkout",
    .class = .obvious_spike,
    .esql_digest = "sha256:0f3a",
    .detected_offset_ms = SPIKE_OFFSET_MS + 120_000,
    .actionable_offset_ms = SPIKE_OFFSET_MS + 240_000,
};

test "test_injector_deterministic" {
    const first = try injector.corpusHashHex(evalManifest());
    const second = try injector.corpusHashHex(evalManifest());
    try testing.expectEqualStrings(&first, &second);

    // One changed field in one seed must change the corpus hash.
    var mutated_seeds = EVAL_SEEDS;
    mutated_seeds[0].magnitude_pct += 1;
    const mutated = manifest.SeedManifest{ .set = .evaluation, .epoch_ms = EPOCH_MS, .seeds = &mutated_seeds };
    const third = try injector.corpusHashHex(mutated);
    try testing.expect(!std.mem.eql(u8, &first, &third));
}

test "test_seed_manifests_disjoint" {
    // A calibration set that reuses an evaluation id violates the split.
    const overlapping_seeds = [_]manifest.Seed{
        .{ .id = "eval-spike-01", .class = .obvious_spike, .service = "checkout", .offset_ms = 0, .duration_ms = 300_000, .magnitude_pct = 30 },
    };
    const overlapping = manifest.SeedManifest{ .set = .calibration, .epoch_ms = EPOCH_MS, .seeds = &overlapping_seeds };
    try testing.expectError(error.OverlappingSeedIds, manifest.assertDisjoint(evalManifest(), overlapping));

    // The scorer refuses through the same check — a mixed corpus never scores.
    var hash = baseline.configHashHex(BASELINE);
    const freeze = baseline.Freeze{ .tuned_on = .calibration, .config_hash = &hash };
    try testing.expectError(
        error.OverlappingSeedIds,
        scoring.score(testing.allocator, evalManifest(), overlapping, BASELINE, freeze, &.{}),
    );
}

test "test_baseline_frozen" {
    var hash = baseline.configHashHex(BASELINE);
    const freeze = baseline.Freeze{ .tuned_on = .calibration, .config_hash = &hash };
    try baseline.verifyFrozen(BASELINE, freeze);

    // Any post-calibration threshold drift is refused.
    var drifted = BASELINE;
    drifted.error_rate_pct_max += 1;
    try testing.expectError(error.BaselineDrifted, baseline.verifyFrozen(drifted, freeze));
    try testing.expectError(
        error.BaselineDrifted,
        scoring.score(testing.allocator, evalManifest(), calibManifest(), drifted, freeze, &.{}),
    );

    // A baseline tuned on the evaluation set is refused outright.
    const eval_tuned = baseline.Freeze{ .tuned_on = .evaluation, .config_hash = &hash };
    try testing.expectError(error.TunedOnEvaluation, baseline.verifyFrozen(BASELINE, eval_tuned));
}

test "test_scoring_requires_service_and_class" {
    var hash = baseline.configHashHex(BASELINE);
    const freeze = baseline.Freeze{ .tuned_on = .calibration, .config_hash = &hash };

    // An unstructured "anomaly found" claim: digest and timestamp but no
    // service, no class. Scores zero and counts as a false positive.
    const unstructured = scoring.Finding{
        .esql_digest = "sha256:beef",
        .detected_offset_ms = SPIKE_OFFSET_MS + 60_000,
    };
    const vague_runs = [_]scoring.Run{.{ .findings = &.{unstructured} }};
    var vague = try scoring.score(testing.allocator, evalManifest(), calibManifest(), BASELINE, freeze, &vague_runs);
    defer vague.deinit(testing.allocator);
    try testing.expectEqual(@as(u32, 0), vague.detected_incidents);
    try testing.expectEqual(@as(u32, 1), vague.false_positives);

    // The same claim WITH service+class but no cited digest still scores zero.
    var no_digest = STRUCTURED_SPIKE_FINDING;
    no_digest.esql_digest = null;
    const no_digest_runs = [_]scoring.Run{.{ .findings = &.{no_digest} }};
    var undigested = try scoring.score(testing.allocator, evalManifest(), calibManifest(), BASELINE, freeze, &no_digest_runs);
    defer undigested.deinit(testing.allocator);
    try testing.expectEqual(@as(u32, 0), undigested.detected_incidents);

    // The fully structured claim scores.
    const good_runs = [_]scoring.Run{.{ .findings = &.{STRUCTURED_SPIKE_FINDING} }};
    var good = try scoring.score(testing.allocator, evalManifest(), calibManifest(), BASELINE, freeze, &good_runs);
    defer good.deinit(testing.allocator);
    try testing.expectEqual(@as(u32, 1), good.detected_incidents);
    try testing.expectEqual(@as(u32, 0), good.false_positives);
}

test "test_report_metrics_complete" {
    var hash = baseline.configHashHex(BASELINE);
    const freeze = baseline.Freeze{ .tuned_on = .calibration, .config_hash = &hash };
    const runs = [_]scoring.Run{
        .{ .findings = &.{STRUCTURED_SPIKE_FINDING}, .query_cost_usd_micros = 1_200, .model_cost_usd_micros = 30_000 },
        .{ .findings = &.{STRUCTURED_SPIKE_FINDING} },
    };
    var r = try scoring.score(testing.allocator, evalManifest(), calibManifest(), BASELINE, freeze, &runs);
    defer r.deinit(testing.allocator);

    // eval-spike-01 (magnitude 40 > threshold 25) is the honest threshold win.
    try testing.expect(r.threshold_wins.len >= 1);

    const json = try report.emitJson(testing.allocator, r);
    defer testing.allocator.free(json);
    // Every metric the report struct declares must appear in the emitted JSON;
    // the field list IS the completeness contract, so iterate it.
    inline for (std.meta.fields(report.Report)) |field| {
        try testing.expect(std.mem.indexOf(u8, json, field.name) != null);
    }
}
